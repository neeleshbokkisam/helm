use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::broadcast;
use tracing::error;

use helm_core::{
    Module, ModuleContext, ModuleError, ModuleTopics, module_topics, topics,
};

use crate::snapshot::TickSnapshot;

pub const BROADCAST_CAPACITY: usize = 64;

pub struct DashboardConfig {
    pub port: u16,
    pub static_dir: PathBuf,
}

impl DashboardConfig {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            static_dir: default_static_dir(),
        }
    }

    pub fn with_static_dir(mut self, static_dir: PathBuf) -> Self {
        self.static_dir = static_dir;
        self
    }
}

pub fn default_static_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("frontend/dist")
}

pub struct DashboardModule {
    config: DashboardConfig,
}

impl DashboardModule {
    pub fn new(config: DashboardConfig) -> Self {
        Self { config }
    }
}

pub(crate) fn push_snapshot(
    tx: &broadcast::Sender<String>,
    snapshot: TickSnapshot,
) {
    let Some(json) = snapshot.to_json() else {
        return;
    };
    let _ = tx.send(json);
}

pub(crate) async fn run_bus_loop(
    ctx: ModuleContext,
    tx: Option<broadcast::Sender<String>>,
) -> Result<(), ModuleError> {
    let mut tick_rx = ctx.bus.subscribe_watch(&topics::TICK)?;
    let state_rx = ctx.bus.subscribe_watch(&topics::CART_POLE_STATE)?;
    let force_safe_rx = ctx.bus.subscribe_watch(&topics::FORCE_CMD_SAFE)?;
    let safety_rx = ctx.bus.subscribe_watch(&topics::SAFETY_STATUS)?;

    loop {
        tokio::select! {
            _ = ctx.shutdown.cancelled() => break,
            changed = tick_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                let timestamp = tick_rx.borrow_and_update().timestamp;
                let snapshot = TickSnapshot::new(
                    timestamp,
                    *state_rx.borrow(),
                    force_safe_rx.borrow().force_n,
                    *safety_rx.borrow(),
                );
                if let Some(tx) = tx.as_ref() {
                    push_snapshot(tx, snapshot);
                }
            }
        }
    }

    Ok(())
}

#[async_trait]
impl Module for DashboardModule {
    fn name(&self) -> &'static str {
        "dashboard"
    }

    fn topics(&self) -> ModuleTopics {
        module_topics! {
            sub: [
                topics::TICK,
                topics::CART_POLE_STATE,
                topics::FORCE_CMD_SAFE,
                topics::SAFETY_STATUS,
            ],
            publish: [],
        }
    }

    async fn run(&self, ctx: ModuleContext) -> Result<(), ModuleError> {
        let tx = match crate::server::try_start_server(
            self.config.port,
            self.config.static_dir.clone(),
            ctx.shutdown.clone(),
        )
        .await
        {
            Ok(tx) => Some(tx),
            Err(e) => {
                error!(
                    "dashboard: failed to bind port {} — live UI disabled; control loop continues ({e})",
                    self.config.port
                );
                None
            }
        };

        run_bus_loop(ctx, tx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helm_core::{Runtime, TopicBus, Timestamp};

    fn register_all(bus: &mut TopicBus) {
        bus.register(&topics::TICK).unwrap();
        bus.register(&topics::CART_POLE_STATE).unwrap();
        bus.register(&topics::FORCE_CMD_SAFE).unwrap();
        bus.register(&topics::SAFETY_STATUS).unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn bus_loop_pushes_json_on_tick() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let (tx, mut rx) = broadcast::channel(BROADCAST_CAPACITY);

        let mut runtime = Runtime::new(handle.clone());
        let ctx_bus = runtime.bus();
        let shutdown = tokio_util::sync::CancellationToken::new();
        let topics = DashboardModule::new(DashboardConfig::new(0)).topics();
        let ctx = ModuleContext {
            bus: helm_core::ModuleBus::new(ctx_bus, topics),
            shutdown: shutdown.clone(),
        };

        let loop_handle = tokio::spawn(async move { run_bus_loop(ctx, Some(tx)).await });

        let run = tokio::spawn(async move {
            runtime.run_for_ticks(3, Duration::from_millis(10)).await
        });

        for _ in 0..3 {
            tokio::time::advance(Duration::from_millis(10)).await;
            tokio::task::yield_now().await;
        }

        run.await.unwrap().unwrap();
        shutdown.cancel();
        loop_handle.await.unwrap().unwrap();

        let json = rx.recv().await.unwrap();
        assert!(json.contains("\"tick\":"));
    }

    #[tokio::test(start_paused = true)]
    async fn bus_loop_without_sender_is_noop() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let mut runtime = Runtime::new(handle.clone());
        let ctx_bus = runtime.bus();
        let shutdown = tokio_util::sync::CancellationToken::new();
        let topics = DashboardModule::new(DashboardConfig::new(0)).topics();
        let ctx = ModuleContext {
            bus: helm_core::ModuleBus::new(ctx_bus, topics),
            shutdown: shutdown.clone(),
        };

        let loop_handle = tokio::spawn(async move { run_bus_loop(ctx, None).await });

        let run = tokio::spawn(async move {
            runtime.run_for_ticks(2, Duration::from_millis(10)).await
        });

        for _ in 0..2 {
            tokio::time::advance(Duration::from_millis(10)).await;
            tokio::task::yield_now().await;
        }

        run.await.unwrap().unwrap();
        shutdown.cancel();
        loop_handle.await.unwrap().unwrap();
    }

    #[test]
    fn push_snapshot_ignores_no_receivers() {
        let (tx, _rx) = broadcast::channel(BROADCAST_CAPACITY);
        drop(_rx);
        push_snapshot(
            &tx,
            TickSnapshot::new(
                Timestamp {
                    tick: 1,
                    dt_secs: 0.01,
                },
                topics::CART_POLE_STATE.seed,
                0.0,
                topics::SAFETY_STATUS.seed,
            ),
        );
    }
}
