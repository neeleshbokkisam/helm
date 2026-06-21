use std::sync::{Arc, Mutex};
use std::time::Duration;

use helm_core::{Runtime, TopicBus, topics};
use helm_modules::StabilizerModule;
use helm_sim::CartPoleModule;

fn register_all(bus: &mut TopicBus) {
    bus.register(&topics::TICK).unwrap();
    bus.register(&topics::CART_POLE_STATE).unwrap();
    bus.register(&topics::FORCE_CMD).unwrap();
}

#[tokio::test(start_paused = true)]
async fn cart_pole_stays_bounded_after_settling() {
    let (mut topic_bus, bus) = TopicBus::new();
    register_all(&mut topic_bus);

    let mut runtime = Runtime::new(bus.clone());
    runtime
        .add_module(Box::new(CartPoleModule::new()))
        .unwrap();
    runtime.add_module(Box::new(StabilizerModule)).unwrap();

    let thetas = Arc::new(Mutex::new(Vec::new()));
    let thetas_rec = Arc::clone(&thetas);

    let bus_rec = bus.clone();
    let recorder = tokio::spawn(async move {
        let mut tick_rx = bus_rec.subscribe_watch(&topics::TICK).unwrap();
        let state_rx = bus_rec.subscribe_watch(&topics::CART_POLE_STATE).unwrap();

        while tick_rx.changed().await.is_ok() {
            let tick = tick_rx.borrow_and_update().timestamp.tick;
            thetas_rec
                .lock()
                .unwrap()
                .push((tick, state_rx.borrow().theta));
            if tick >= 500 {
                break;
            }
        }
    });

    let dt = Duration::from_millis(10);
    let run_handle = tokio::spawn(async move { runtime.run_for_ticks(500, dt).await });

    for _ in 0..500 {
        tokio::time::advance(dt).await;
        tokio::task::yield_now().await;
    }

    run_handle.await.unwrap().unwrap();
    recorder.await.unwrap();

    let samples = thetas.lock().unwrap();
    let window: Vec<_> = samples
        .iter()
        .filter(|(tick, _)| *tick >= 50 && *tick <= 500)
        .collect();

    assert!(!window.is_empty());
    for (_, theta) in &window {
        assert!(theta.abs() < 0.3, "theta {theta} out of bounds");
    }

    let max = window
        .iter()
        .map(|(_, t)| t.abs())
        .fold(0.0_f64, f64::max);
    assert!(max < 0.25, "max theta {max}");
}

#[tokio::test(start_paused = true)]
async fn cart_pole_deterministic() {
    async fn run_once() -> f64 {
        let (mut topic_bus, bus) = TopicBus::new();
        register_all(&mut topic_bus);

        let mut runtime = Runtime::new(bus.clone());
        runtime
            .add_module(Box::new(CartPoleModule::new()))
            .unwrap();
        runtime.add_module(Box::new(StabilizerModule)).unwrap();

        let dt = Duration::from_millis(10);
        let run_handle = tokio::spawn(async move { runtime.run_for_ticks(200, dt).await });

        for _ in 0..200 {
            tokio::time::advance(dt).await;
            tokio::task::yield_now().await;
        }

        run_handle.await.unwrap().unwrap();

        let state_rx = bus.subscribe_watch(&topics::CART_POLE_STATE).unwrap();
        let theta = state_rx.borrow().theta;
        theta
    }

    let a = run_once().await;
    let b = run_once().await;
    assert_eq!(a, b);
}
