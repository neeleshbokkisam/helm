use std::path::PathBuf;
use std::time::Duration;

use helm_core::{Module, ModuleBus, ModuleContext, Runtime, Timestamp, TopicBus, topics};
use helm_dashboard::{
    BROADCAST_CAPACITY, DashboardConfig, DashboardModule, TickSnapshot, push_snapshot,
    run_bus_loop, try_start_server,
};
use tokio_util::sync::CancellationToken;
use tokio_tungstenite::connect_async;

fn register_all(bus: &mut TopicBus) {
    bus.register(&topics::TICK).unwrap();
    bus.register(&topics::CART_POLE_STATE).unwrap();
    bus.register(&topics::FORCE_CMD_SAFE).unwrap();
    bus.register(&topics::SAFETY_STATUS).unwrap();
}

/// Paused-time, fully virtual: lagging broadcast receiver only (no TCP).
/// Virtual tick count can be large; wall time should stay well under one second.
#[tokio::test(start_paused = true)]
async fn lagging_broadcast_does_not_block_bus_loop_at_many_ticks() {
    const TICKS: u64 = 5000;
    const DT: Duration = Duration::from_millis(10);

    let wall_start = std::time::Instant::now();

    let (mut bus, handle) = TopicBus::new();
    register_all(&mut bus);

    let (tx, _active) = tokio::sync::broadcast::channel(BROADCAST_CAPACITY);
    let _lagging = tx.subscribe();

    let mut runtime = Runtime::new(handle.clone());
    let shutdown = CancellationToken::new();
    let topics = DashboardModule::new(DashboardConfig::new(0)).topics();
    let ctx = ModuleContext {
        bus: ModuleBus::new(runtime.bus(), topics),
        shutdown: shutdown.clone(),
    };

    let bus_loop = tokio::spawn(async move { run_bus_loop(ctx, Some(tx)).await });

    let ticks = tokio::spawn(async move { runtime.run_for_ticks(TICKS, DT).await });

    for _ in 0..TICKS {
        tokio::time::advance(DT).await;
        tokio::task::yield_now().await;
    }

    ticks.await.unwrap().unwrap();
    shutdown.cancel();
    bus_loop.await.unwrap().unwrap();

    assert!(
        wall_start.elapsed() < Duration::from_secs(5),
        "paused virtual test took too long in wall time: {:?} (expected << 1s normally)",
        wall_start.elapsed()
    );
}

/// Real wall-clock time with a silent TCP WebSocket client that never reads.
#[tokio::test]
async fn slow_ws_client_does_not_block_bus_loop_wall_clock() {
    let (mut bus, handle) = TopicBus::new();
    register_all(&mut bus);

    let shutdown = CancellationToken::new();
    let server = try_start_server(
        0,
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("frontend/dist"),
        shutdown.clone(),
    )
    .await
    .unwrap();

    let url = format!("ws://{}/ws", server.addr);
    let _slow_client = tokio::spawn(async move {
        let (ws, _) = connect_async(&url).await.unwrap();
        std::future::pending::<()>().await;
        drop(ws);
    });

    for _ in 0..20 {
        tokio::task::yield_now().await;
    }

    let mut runtime = Runtime::new(handle.clone());
    let topics = DashboardModule::new(DashboardConfig::new(0)).topics();
    let ctx = ModuleContext {
        bus: ModuleBus::new(runtime.bus(), topics),
        shutdown: shutdown.clone(),
    };

    let bus_loop = tokio::spawn(async move { run_bus_loop(ctx, Some(server.tx)).await });

    const TICKS: u64 = 1000;
    const DT: Duration = Duration::from_millis(10);
    let ticks = tokio::spawn(async move { runtime.run_for_ticks(TICKS, DT).await });

    let started = std::time::Instant::now();
    ticks.await.unwrap().unwrap();
    shutdown.cancel();
    bus_loop.await.unwrap().unwrap();

    let elapsed = started.elapsed();
    assert!(
        elapsed <= Duration::from_secs(15),
        "bus loop blocked with slow ws client: took {elapsed:?} for {TICKS} ticks"
    );
    assert!(
        elapsed >= Duration::from_secs(9),
        "finished suspiciously fast for real-time ticks: {elapsed:?}"
    );
}

#[test]
fn push_snapshot_never_blocks_with_lagging_receivers() {
    let (tx, _rx) = tokio::sync::broadcast::channel(4);
    let _slow_a = tx.subscribe();
    let _slow_b = tx.subscribe();

    let snap = TickSnapshot::new(
        Timestamp {
            tick: 0,
            dt_secs: 0.01,
        },
        topics::CART_POLE_STATE.seed,
        0.0,
        topics::SAFETY_STATUS.seed,
    );

    for tick in 1..=10_000 {
        push_snapshot(
            &tx,
            TickSnapshot::new(
                Timestamp {
                    tick,
                    dt_secs: 0.01,
                },
                snap.state,
                snap.force_safe_n,
                snap.safety,
            ),
        );
    }
}
