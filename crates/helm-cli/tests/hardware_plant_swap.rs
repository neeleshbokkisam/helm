use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use helm_core::{Runtime, TopicBus, topics};
use helm_hardware::{HardwareConfig, HardwarePlantModule, HOST_RESERVE_MS};
use helm_modules::{SafetyConfig, SafetyModule, StabilizerModule};
use helm_sim::CartPoleModule;

const THETA_EPS: f64 = 1e-4;
/// Early closed-loop transient: hw stabilizer tick branch reads pre-roundtrip state while
/// sim stabilizer is state-triggered on post-integration float state. Decays by tick ~50.
const STARTUP_THETA_EPS: f64 = 3.5e-3;

#[derive(Clone)]
struct Sample {
    tick: u64,
    theta: f64,
}

fn register_all(bus: &mut TopicBus) {
    bus.register(&topics::TICK).unwrap();
    bus.register(&topics::CART_POLE_STATE).unwrap();
    bus.register(&topics::FORCE_CMD).unwrap();
    bus.register(&topics::FORCE_CMD_SAFE).unwrap();
    bus.register(&topics::SAFETY_STATUS).unwrap();
}

async fn run_sim_reference(ticks: u64, dt_ms: u64) -> Vec<Sample> {
    let (mut bus, handle) = TopicBus::new();
    register_all(&mut bus);

    let mut runtime = Runtime::new(handle.clone());
    runtime
        .add_module(Box::new(StabilizerModule::new()))
        .unwrap();
    runtime
        .add_module(Box::new(SafetyModule::new(SafetyConfig::new(dt_ms))))
        .unwrap();
    runtime
        .add_module(Box::new(CartPoleModule::new()))
        .unwrap();

    let samples = Arc::new(Mutex::new(Vec::new()));
    let samples_rec = Arc::clone(&samples);
    let bus_rec = handle.clone();
    let recorder = tokio::spawn(async move {
        let mut tick_rx = bus_rec.subscribe_watch(&topics::TICK).unwrap();
        let state_rx = bus_rec.subscribe_watch(&topics::CART_POLE_STATE).unwrap();
        while tick_rx.changed().await.is_ok() {
            let tick = tick_rx.borrow_and_update().timestamp.tick;
            samples_rec.lock().unwrap().push(Sample {
                tick,
                theta: state_rx.borrow().theta,
            });
            if tick >= ticks {
                break;
            }
        }
    });

    let dt = Duration::from_millis(dt_ms);
    tokio::time::sleep(Duration::from_millis(150)).await;
    runtime.run_for_ticks(ticks, dt).await.unwrap();
    recorder.await.unwrap();
    let out = samples.lock().unwrap().clone();
    out
}

async fn run_hardware_stack(ticks: u64, dt_ms: u64) -> Vec<Sample> {
    let (mut bus, handle) = TopicBus::new();
    register_all(&mut bus);

    let plant = HardwarePlantModule::new(HardwareConfig::new(dt_ms))
        .with_spawned_device()
        .unwrap();

    let mut runtime = Runtime::new(handle.clone());
    runtime
        .add_module(Box::new(StabilizerModule::new()))
        .unwrap();
    runtime
        .add_module(Box::new(SafetyModule::new(SafetyConfig::new(dt_ms))))
        .unwrap();
    runtime.add_module(Box::new(plant)).unwrap();

    let samples = Arc::new(Mutex::new(Vec::new()));
    let samples_rec = Arc::clone(&samples);
    let bus_rec = handle.clone();
    let recorder = tokio::spawn(async move {
        let mut tick_rx = bus_rec.subscribe_watch(&topics::TICK).unwrap();
        let state_rx = bus_rec.subscribe_watch(&topics::CART_POLE_STATE).unwrap();
        while tick_rx.changed().await.is_ok() {
            let tick = tick_rx.borrow_and_update().timestamp.tick;
            samples_rec.lock().unwrap().push(Sample {
                tick,
                theta: state_rx.borrow().theta,
            });
            if tick >= ticks {
                break;
            }
        }
    });

    let dt = Duration::from_millis(dt_ms);
    tokio::time::sleep(Duration::from_millis(150)).await;
    runtime.run_for_ticks(ticks, dt).await.unwrap();
    recorder.await.unwrap();
    let out = samples.lock().unwrap().clone();
    out
}

/// Upper-stack latency: TICK edge → FORCE_CMD_SAFE update on the same tick.
pub async fn measure_host_reserve_ms(ticks: u64, dt_ms: u64) -> f64 {
    let (mut bus, handle) = TopicBus::new();
    register_all(&mut bus);

    let plant = HardwarePlantModule::new(HardwareConfig::new(dt_ms))
        .with_spawned_device()
        .unwrap();

    let mut runtime = Runtime::new(handle.clone());
    runtime
        .add_module(Box::new(StabilizerModule::new()))
        .unwrap();
    runtime
        .add_module(Box::new(SafetyModule::new(SafetyConfig::new(dt_ms))))
        .unwrap();
    runtime.add_module(Box::new(plant)).unwrap();

    tokio::time::sleep(Duration::from_millis(150)).await;

    let max_us = Arc::new(Mutex::new(0u128));
    let max_us_rec = Arc::clone(&max_us);
    let bus_rec = handle.clone();
    let recorder = tokio::spawn(async move {
        let mut tick_rx = bus_rec.subscribe_watch(&topics::TICK).unwrap();
        let mut force_rx = bus_rec.subscribe_watch(&topics::FORCE_CMD_SAFE).unwrap();
        let mut last_tick = 0u64;
        let mut t0 = Instant::now();

        while tick_rx.changed().await.is_ok() {
            let tick = tick_rx.borrow_and_update().timestamp.tick;
            if tick != last_tick {
                last_tick = tick;
                t0 = Instant::now();
            }
            if force_rx.has_changed().unwrap_or(false) {
                let _ = force_rx.borrow_and_update();
                if tick == last_tick {
                    let us = t0.elapsed().as_micros();
                    let mut guard = max_us_rec.lock().unwrap();
                    if us > *guard {
                        *guard = us;
                    }
                }
            }
            if tick >= ticks {
                break;
            }
        }
    });

    let dt = Duration::from_millis(dt_ms);
    runtime.run_for_ticks(ticks, dt).await.unwrap();
    recorder.await.unwrap();
    let guard = max_us.lock().unwrap();
    *guard as f64 / 1000.0
}

#[tokio::test]
async fn hardware_stack_stays_bounded() {
    let dt_ms = 10;
    let ticks = 500;
    let hw = run_hardware_stack(ticks, dt_ms).await;

    let window: Vec<_> = hw.iter().filter(|s| s.tick >= 100).collect();
    assert!(!window.is_empty());
    let max_theta = window
        .iter()
        .map(|s| s.theta.abs())
        .fold(0.0_f64, f64::max);
    assert!(max_theta < 0.3, "max theta {max_theta}");
}

#[tokio::test]
async fn hardware_trajectory_is_reproducible() {
    let dt_ms = 10;
    let ticks = 200;
    let a = run_hardware_stack(ticks, dt_ms).await;
    let b = run_hardware_stack(ticks, dt_ms).await;

    let map_b: HashMap<_, _> = b.iter().map(|s| (s.tick, s.theta)).collect();
    for sample in a.iter().filter(|s| s.tick >= 50) {
        let theta_b = map_b.get(&sample.tick).expect("missing tick in second run");
        assert!(
            (sample.theta - theta_b).abs() < THETA_EPS,
            "tick {} a={} b={}",
            sample.tick,
            sample.theta,
            theta_b
        );
    }
}

#[tokio::test]
async fn hardware_matches_sim_within_wire_quantization() {
    let dt_ms = 10;
    let ticks = 500;
    let sim = run_sim_reference(ticks, dt_ms).await;
    let hw = run_hardware_stack(ticks, dt_ms).await;

    let sim_final = sim.last().expect("sim samples").theta;
    let hw_final = hw.last().expect("hw samples").theta;
    assert!(
        (sim_final - hw_final).abs() < THETA_EPS,
        "final theta sim={sim_final} hw={hw_final}"
    );

    let sim_map: HashMap<_, _> = sim.iter().map(|s| (s.tick, s.theta)).collect();
    let mut compared = 0usize;
    let mut max_delta = 0.0_f64;
    for sample in hw.iter().filter(|s| s.tick >= 50 && s.tick < 400) {
        let Some(sim_theta) = sim_map.get(&sample.tick) else {
            continue;
        };
        let delta = (sample.theta - sim_theta).abs();
        max_delta = max_delta.max(delta);
        assert!(
            delta < STARTUP_THETA_EPS,
            "tick {} hw={} sim={} delta={}",
            sample.tick,
            sample.theta,
            sim_theta,
            delta
        );
        compared += 1;
    }
    assert!(compared > 100, "expected startup-window overlap");

    max_delta = 0.0;
    for sample in hw.iter().filter(|s| s.tick >= 400) {
        let Some(sim_theta) = sim_map.get(&sample.tick) else {
            continue;
        };
        let delta = (sample.theta - sim_theta).abs();
        max_delta = max_delta.max(delta);
        assert!(
            delta < THETA_EPS,
            "tick {} hw={} sim={} delta={}",
            sample.tick,
            sample.theta,
            sim_theta,
            delta
        );
        compared += 1;
    }
    assert!(compared > 150, "expected full-window overlap");
    assert!(max_delta < THETA_EPS, "max late delta {max_delta}");
}

#[tokio::test]
async fn host_reserve_fits_scheduling_budget() {
    let dt_ms = 10;
    let ticks = 200;
    let max_ms = measure_host_reserve_ms(ticks, dt_ms).await;
    eprintln!(
        "measured upper-stack tick margin: {max_ms:.3} ms (HOST_RESERVE_MS={HOST_RESERVE_MS})"
    );
    assert!(
        max_ms < HOST_RESERVE_MS as f64,
        "upper stack took {max_ms} ms, reserve is {HOST_RESERVE_MS} ms"
    );
}
