use std::time::Duration;

use helm_core::{Runtime, TopicBus, topics};
use helm_hardware::{HardwareConfig, HardwarePlantModule};

pub fn register_all(bus: &mut TopicBus) {
    bus.register(&topics::TICK).unwrap();
    bus.register(&topics::CART_POLE_STATE).unwrap();
    bus.register(&topics::FORCE_CMD).unwrap();
    bus.register(&topics::FORCE_CMD_SAFE).unwrap();
    bus.register(&topics::SAFETY_STATUS).unwrap();
}

#[tokio::test]
async fn hardware_plant_publishes_over_pty() {
    let (mut bus, handle) = TopicBus::new();
    register_all(&mut bus);

    let plant = HardwarePlantModule::new(HardwareConfig::new(10))
        .with_spawned_device()
        .unwrap();

    let mut runtime = Runtime::new(handle.clone());
    runtime.add_module(Box::new(plant)).unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    let run = tokio::spawn(async move {
        runtime
            .run_for_ticks(20, Duration::from_millis(10))
            .await
    });

    let state_rx = handle.subscribe_watch(&topics::CART_POLE_STATE).unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if state_rx.borrow().theta != topics::CART_POLE_STATE.seed.theta {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("hardware plant never published state");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    run.await.unwrap().unwrap();
}
