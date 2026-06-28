//! Diagnostic: sim vs hardware theta delta profile (not a pass/fail gate).
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::time::Duration;

use helm_core::{Runtime, TopicBus, topics};
use helm_hardware::{HardwareConfig, HardwarePlantModule};
use helm_modules::{SafetyConfig, SafetyModule, StabilizerModule};
use helm_sim::CartPoleModule;

#[derive(Clone, Copy)]
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

async fn run_sim(ticks: u64, dt_ms: u64) -> Vec<Sample> {
    let (mut bus, handle) = TopicBus::new();
    register_all(&mut bus);
    let mut runtime = Runtime::new(handle.clone());
    runtime.add_module(Box::new(StabilizerModule::new())).unwrap();
    runtime
        .add_module(Box::new(SafetyModule::new(SafetyConfig::new(dt_ms))))
        .unwrap();
    runtime.add_module(Box::new(CartPoleModule::new())).unwrap();

    let mut out = Vec::new();
    let bus_rec = handle.clone();
    let recorder = tokio::spawn(async move {
        let mut tick_rx = bus_rec.subscribe_watch(&topics::TICK).unwrap();
        let state_rx = bus_rec.subscribe_watch(&topics::CART_POLE_STATE).unwrap();
        while tick_rx.changed().await.is_ok() {
            let tick = tick_rx.borrow_and_update().timestamp.tick;
            out.push(Sample {
                tick,
                theta: state_rx.borrow().theta,
            });
            if tick >= ticks {
                break;
            }
        }
        out
    });

    tokio::time::sleep(Duration::from_millis(150)).await;
    runtime
        .run_for_ticks(ticks, Duration::from_millis(dt_ms))
        .await
        .unwrap();
    recorder.await.unwrap()
}

async fn run_hw(ticks: u64, dt_ms: u64) -> Vec<Sample> {
    let (mut bus, handle) = TopicBus::new();
    register_all(&mut bus);
    let plant = HardwarePlantModule::new(HardwareConfig::new(dt_ms))
        .with_spawned_device()
        .unwrap();
    let mut runtime = Runtime::new(handle.clone());
    runtime.add_module(Box::new(StabilizerModule::new())).unwrap();
    runtime
        .add_module(Box::new(SafetyModule::new(SafetyConfig::new(dt_ms))))
        .unwrap();
    runtime.add_module(Box::new(plant)).unwrap();

    let mut out = Vec::new();
    let bus_rec = handle.clone();
    let recorder = tokio::spawn(async move {
        let mut tick_rx = bus_rec.subscribe_watch(&topics::TICK).unwrap();
        let state_rx = bus_rec.subscribe_watch(&topics::CART_POLE_STATE).unwrap();
        while tick_rx.changed().await.is_ok() {
            let tick = tick_rx.borrow_and_update().timestamp.tick;
            out.push(Sample {
                tick,
                theta: state_rx.borrow().theta,
            });
            if tick >= ticks {
                break;
            }
        }
        out
    });

    tokio::time::sleep(Duration::from_millis(150)).await;
    runtime
        .run_for_ticks(ticks, Duration::from_millis(dt_ms))
        .await
        .unwrap();
    recorder.await.unwrap()
}

#[tokio::test]
async fn dump_sim_hw_theta_delta_profile() {
    let dt_ms = 10;
    let ticks = 500;
    let sim = run_sim(ticks, dt_ms).await;
    let hw = run_hw(ticks, dt_ms).await;

    let sim_map: HashMap<_, _> = sim.iter().map(|s| (s.tick, s.theta)).collect();

    let path = std::env::temp_dir().join("helm_sim_hw_delta.csv");
    let mut f = File::create(&path).unwrap();
    writeln!(f, "tick,theta_sim,theta_hw,delta,delta_step").unwrap();

    let mut prev_delta = 0.0_f64;
    let mut max_abs_delta = 0.0_f64;
    let mut max_abs_step = 0.0_f64;
    let mut step_jumps = 0usize;

    for h in &hw {
        let Some(&theta_sim) = sim_map.get(&h.tick) else {
            continue;
        };
        let delta = h.theta - theta_sim;
        let delta_step = delta - prev_delta;
        prev_delta = delta;

        if delta.abs() > max_abs_delta {
            max_abs_delta = delta.abs();
        }
        if delta_step.abs() > max_abs_step {
            max_abs_step = delta_step.abs();
        }
        if delta_step.abs() > 0.01 {
            step_jumps += 1;
        }

        writeln!(
            f,
            "{},{:.9},{:.9},{:.9},{:.9}",
            h.tick, theta_sim, h.theta, delta, delta_step
        )
        .unwrap();
    }

    let late: Vec<_> = hw
        .iter()
        .filter(|s| s.tick >= 400)
        .filter_map(|s| sim_map.get(&s.tick).map(|sim| (s.theta - sim).abs()))
        .collect();
    let late_max = late.iter().copied().fold(0.0_f64, f64::max);
    let late_mean = late.iter().sum::<f64>() / late.len() as f64;

    eprintln!("csv: {}", path.display());
    eprintln!("max |delta| all ticks: {max_abs_delta:.9}");
    eprintln!("max |delta_step| all ticks: {max_abs_step:.9}");
    eprintln!("step jumps |delta_step|>0.01: {step_jumps}");
    eprintln!("late window tick>=400: max={late_max:.9} mean={late_mean:.9} n={}", late.len());

    // Always pass — diagnostic only
    assert!(max_abs_delta.is_finite());
}
