//! 120 s hardware soak: no spurious StateStale/CommandStale during steady-state plateaus.
use std::time::Duration;

use helm_core::{Runtime, SafetyFault, TopicBus, topics};
use helm_hardware::{HardwareConfig, HardwarePlantModule};
use helm_modules::{SafetyConfig, SafetyModule, StabilizerModule};

fn register_all(bus: &mut TopicBus) {
    bus.register(&topics::TICK).unwrap();
    bus.register(&topics::CART_POLE_STATE).unwrap();
    bus.register(&topics::FORCE_CMD).unwrap();
    bus.register(&topics::FORCE_CMD_SAFE).unwrap();
    bus.register(&topics::SAFETY_STATUS).unwrap();
}

#[tokio::test]
async fn hardware_120s_no_spurious_stale_faults() {
    let dt_ms = 10;
    let seconds = 120;
    let ticks = seconds * 1000 / dt_ms;

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

    let bus_rec = handle.clone();
    let monitor = tokio::spawn(async move {
        let mut tick_rx = bus_rec.subscribe_watch(&topics::TICK).unwrap();
        let mut state_rx = bus_rec.subscribe_watch(&topics::CART_POLE_STATE).unwrap();
        let status_rx = bus_rec.subscribe_watch(&topics::SAFETY_STATUS).unwrap();
        let force_safe_rx = bus_rec.subscribe_watch(&topics::FORCE_CMD_SAFE).unwrap();

        let mut max_quiet_state_ticks = 0u64;
        let mut quiet_state_ticks = 0u64;
        let mut max_identical_borrow_streak = 0u64;
        let mut identical_borrow_streak = 0u64;
        let mut prev_borrow = *state_rx.borrow();
        let mut first_fault = None::<(u64, SafetyFault)>;
        let mut max_theta = 0.0_f64;

        while tick_rx.changed().await.is_ok() {
            let tick = tick_rx.borrow_and_update().timestamp.tick;

            if state_rx.has_changed().unwrap_or(false) {
                let _ = state_rx.borrow_and_update();
                quiet_state_ticks = 0;
            } else {
                quiet_state_ticks += 1;
                if quiet_state_ticks > max_quiet_state_ticks {
                    max_quiet_state_ticks = quiet_state_ticks;
                }
            }

            let borrowed = *state_rx.borrow();
            if borrowed == prev_borrow {
                identical_borrow_streak += 1;
                max_identical_borrow_streak =
                    max_identical_borrow_streak.max(identical_borrow_streak);
            } else {
                identical_borrow_streak = 0;
                prev_borrow = borrowed;
            }

            max_theta = max_theta.max(state_rx.borrow().theta.abs());

            if let Some(fault) = status_rx.borrow().latched_fault {
                if first_fault.is_none() {
                    first_fault = Some((tick, fault));
                }
            }

            let force_safe = force_safe_rx.borrow().force_n;
            if tick >= ticks {
                return (
                    first_fault,
                    max_quiet_state_ticks,
                    max_identical_borrow_streak,
                    max_theta,
                    force_safe,
                    borrowed,
                );
            }
        }

        panic!("tick stream ended early");
    });

    tokio::time::sleep(Duration::from_millis(150)).await;
    runtime
        .run_for_ticks(ticks, Duration::from_millis(dt_ms))
        .await
        .unwrap();

    let (first_fault, max_quiet, max_identical, max_theta, force_safe, last_state) =
        monitor.await.unwrap();

    eprintln!(
        "120s soak: max_theta={max_theta:.6} max_quiet_state_ticks={max_quiet} \
         max_identical_borrow_streak={max_identical} final_force_safe={force_safe:.4} \
         final_theta={:.6}",
        last_state.theta
    );

    if let Some((tick, fault)) = first_fault {
        panic!("spurious safety fault at tick {tick}: {fault:?}");
    }
}
