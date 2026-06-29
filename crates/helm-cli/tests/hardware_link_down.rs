use std::time::Duration;

use helm_core::{SafetyFault, TopicBus, topics};
use helm_hardware::{DeviceFaultConfig, DeviceFaultKind, HardwareConfig, HardwarePlantModule};
use helm_modules::{SafetyConfig, SafetyModule, StabilizerModule};

struct Sample {
    tick: u64,
    force_safe: f64,
    fault: Option<SafetyFault>,
}

fn register_all(bus: &mut TopicBus) {
    bus.register(&topics::TICK).unwrap();
    bus.register(&topics::CART_POLE_STATE).unwrap();
    bus.register(&topics::FORCE_CMD).unwrap();
    bus.register(&topics::FORCE_CMD_SAFE).unwrap();
    bus.register(&topics::SAFETY_STATUS).unwrap();
}

async fn run_link_down_at(fault_at: u32, ticks: u64, dt_ms: u64) -> Vec<Sample> {
    use helm_core::Runtime;

    let (mut bus, handle) = TopicBus::new();
    register_all(&mut bus);

    let mut hw_config = HardwareConfig::new(dt_ms);
    hw_config.device_fault = DeviceFaultConfig {
        kind: Some(DeviceFaultKind::LinkDown),
        at_tick: fault_at,
    };

    let plant = HardwarePlantModule::new(hw_config)
        .with_spawned_device()
        .unwrap();

    let mut runtime = Runtime::new(handle.clone());
    runtime.add_module(Box::new(plant)).unwrap();
    runtime
        .add_module(Box::new(StabilizerModule::new()))
        .unwrap();
    runtime
        .add_module(Box::new(SafetyModule::new(SafetyConfig::new(dt_ms))))
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    let bus_rec = handle.clone();
    let recorder = tokio::spawn(async move { record(bus_rec, ticks).await });

    let dt = Duration::from_millis(dt_ms);
    runtime.run_for_ticks(ticks, dt).await.unwrap();
    recorder.await.unwrap()
}

async fn record(bus: helm_core::BusHandle, max_tick: u64) -> Vec<Sample> {
    let mut out = Vec::new();
    let mut tick_rx = bus.subscribe_watch(&topics::TICK).unwrap();
    let state_rx = bus.subscribe_watch(&topics::CART_POLE_STATE).unwrap();
    let safe_rx = bus.subscribe_watch(&topics::FORCE_CMD_SAFE).unwrap();
    let status_rx = bus.subscribe_watch(&topics::SAFETY_STATUS).unwrap();

    while tick_rx.changed().await.is_ok() {
        let tick = tick_rx.borrow_and_update().timestamp.tick;
        let _ = state_rx.borrow();
        out.push(Sample {
            tick,
            force_safe: safe_rx.borrow().force_n,
            fault: status_rx.borrow().latched_fault,
        });
        if tick >= max_tick {
            break;
        }
    }
    out
}

#[tokio::test]
async fn link_down_latches_state_stale_and_zeros_force() {
    let dt_ms = 10;
    let fault_at = 100u32;
    let samples = run_link_down_at(fault_at, 150, dt_ms).await;

    let pre: Vec<_> = samples.iter().filter(|s| s.tick <= fault_at as u64).collect();
    assert!(pre.windows(2).any(|w| (w[1].force_safe - w[0].force_safe).abs() > 1e-9));

    let post: Vec<_> = samples.iter().filter(|s| s.tick >= 110).collect();
    assert!(post.iter().any(|s| {
        matches!(s.fault, Some(SafetyFault::StateStale { .. }))
    }));
    for s in post.iter().filter(|s| s.fault.is_some()) {
        assert_eq!(s.force_safe, 0.0);
    }
}
