use std::time::Duration;

use helm_core::{BusHandle, FaultConfig, FaultKind, Runtime, SafetyFault, TopicBus, topics};
use helm_modules::{SafetyConfig, SafetyModule, StabilizerModule};
use helm_sim::CartPoleModule;

struct Sample {
    tick: u64,
    force_safe: f64,
    theta: f64,
    fault: Option<SafetyFault>,
}

fn register_all(bus: &mut TopicBus) {
    bus.register(&topics::TICK).unwrap();
    bus.register(&topics::CART_POLE_STATE).unwrap();
    bus.register(&topics::FORCE_CMD).unwrap();
    bus.register(&topics::FORCE_CMD_SAFE).unwrap();
    bus.register(&topics::SAFETY_STATUS).unwrap();
}

fn add_modules(runtime: &mut Runtime, fault: FaultConfig, dt_ms: u64) {
    runtime
        .add_module(Box::new(CartPoleModule::with_fault(fault)))
        .unwrap();
    runtime
        .add_module(Box::new(StabilizerModule::with_fault(fault)))
        .unwrap();
    runtime
        .add_module(Box::new(SafetyModule::new(SafetyConfig::new(dt_ms))))
        .unwrap();
}

async fn run_and_record(fault: FaultConfig, dt_ms: u64, ticks: u64) -> Vec<Sample> {
    let (mut bus, handle) = TopicBus::new();
    register_all(&mut bus);

    let mut runtime = Runtime::new(handle.clone());
    add_modules(&mut runtime, fault, dt_ms);

    let bus_rec = handle.clone();
    let recorder = tokio::spawn(async move { record(bus_rec, ticks).await });

    let dt = Duration::from_millis(dt_ms);
    let run = tokio::spawn(async move { runtime.run_for_ticks(ticks, dt).await });

    for _ in 0..ticks {
        tokio::time::advance(dt).await;
        tokio::task::yield_now().await;
    }

    run.await.unwrap().unwrap();
    recorder.await.unwrap()
}

async fn record(bus: BusHandle, max_tick: u64) -> Vec<Sample> {
    let mut out = Vec::new();
    let mut tick_rx = bus.subscribe_watch(&topics::TICK).unwrap();
    let state_rx = bus.subscribe_watch(&topics::CART_POLE_STATE).unwrap();
    let safe_rx = bus.subscribe_watch(&topics::FORCE_CMD_SAFE).unwrap();
    let status_rx = bus.subscribe_watch(&topics::SAFETY_STATUS).unwrap();

    while tick_rx.changed().await.is_ok() {
        let tick = tick_rx.borrow_and_update().timestamp.tick;
        out.push(Sample {
            tick,
            force_safe: safe_rx.borrow().force_n,
            theta: state_rx.borrow().theta,
            fault: status_rx.borrow().latched_fault,
        });
        if tick >= max_tick {
            break;
        }
    }
    out
}

#[tokio::test(start_paused = true)]
async fn force_overshoot_latched_and_never_forwarded() {
    let fault = FaultConfig {
        kind: Some(FaultKind::ForceOvershoot {
            at_tick: 50,
            force_n: 999.0,
        }),
    };
    let samples = run_and_record(fault, 10, 120).await;

    assert!(samples.iter().all(|s| s.force_safe.abs() <= 20.0));
    assert!(!samples.iter().any(|s| (s.force_safe - 999.0).abs() < 1.0));

    let post = samples.iter().filter(|s| s.tick >= 50).collect::<Vec<_>>();
    assert!(post.iter().any(|s| s.fault.is_some()));
    assert!(post.iter().any(|s| {
        matches!(s.fault, Some(SafetyFault::ForceOutOfRange { .. }))
    }));
    for s in post.iter().filter(|s| s.fault.is_some()) {
        assert_eq!(s.force_safe, 0.0);
    }
}

#[tokio::test(start_paused = true)]
async fn stale_state_latches_and_zeros_force() {
    let fault = FaultConfig {
        kind: Some(FaultKind::StaleState { after_tick: 80 }),
    };
    let samples = run_and_record(fault, 10, 150).await;

    let pre: Vec<_> = samples.iter().filter(|s| s.tick <= 80).collect();
    let post: Vec<_> = samples.iter().filter(|s| s.tick >= 90).collect();
    assert!(pre.windows(2).any(|w| (w[1].theta - w[0].theta).abs() > 1e-9));

    assert!(post.iter().any(|s| {
        matches!(s.fault, Some(SafetyFault::StateStale { .. }))
    }));
    for s in post.iter().filter(|s| s.fault.is_some()) {
        assert_eq!(s.force_safe, 0.0);
    }
}

#[tokio::test(start_paused = true)]
async fn dropped_command_latches_and_zeros_force() {
    let fault = FaultConfig {
        kind: Some(FaultKind::DropCommand { after_tick: 80 }),
    };
    let samples = run_and_record(fault, 10, 150).await;

    let post: Vec<_> = samples.iter().filter(|s| s.tick >= 95).collect();
    assert!(post.iter().any(|s| {
        matches!(s.fault, Some(SafetyFault::CommandStale { .. }))
    }));
    for s in post.iter().filter(|s| s.fault.is_some()) {
        assert_eq!(s.force_safe, 0.0);
    }
}
