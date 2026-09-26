use std::path::PathBuf;
#[cfg(test)]
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::broadcast;
use tracing::error;

use helm_core::{module_topics, topics, Module, ModuleContext, ModuleError, ModuleTopics};

use crate::shared::DashboardShared;
use crate::snapshot::TickSnapshot;
use crate::wire::{ended_json, tick_json};

pub const BROADCAST_CAPACITY: usize = 64;

pub struct DashboardConfig {
    pub port: u16,
    pub static_dir: PathBuf,
    pub session: crate::wire::SessionInfo,
}

impl DashboardConfig {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            static_dir: default_static_dir(),
            session: crate::wire::SessionInfo::live_sim(0.01),
        }
    }

    pub fn with_session(mut self, session: crate::wire::SessionInfo) -> Self {
        self.session = session;
        self
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

pub fn push_snapshot(
    tx: &broadcast::Sender<String>,
    shared: &DashboardShared,
    snapshot: TickSnapshot,
) {
    shared.push_history(snapshot.clone());
    let Some(json) = tick_json(&snapshot) else {
        return;
    };
    let _ = tx.send(json);
}

pub fn send_ended(tx: &broadcast::Sender<String>, final_tick: u64, reason: &str) {
    let Some(json) = ended_json(final_tick, reason) else {
        return;
    };
    let _ = tx.send(json);
}

pub async fn run_bus_loop(
    ctx: ModuleContext,
    tx: Option<broadcast::Sender<String>>,
    shared: Option<DashboardShared>,
) -> Result<(), ModuleError> {
    let mut tick_rx = ctx.bus.subscribe_watch(&topics::TICK)?;
    let state_rx = ctx.bus.subscribe_watch(&topics::CART_POLE_STATE)?;
    let force_safe_rx = ctx.bus.subscribe_watch(&topics::FORCE_CMD_SAFE)?;
    let safety_rx = ctx.bus.subscribe_watch(&topics::SAFETY_STATUS)?;
    let mut last_tick = 0u64;

    loop {
        tokio::select! {
            _ = ctx.shutdown.cancelled() => break,
            changed = tick_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                let timestamp = tick_rx.borrow_and_update().timestamp;
                last_tick = timestamp.tick;
                let snapshot = TickSnapshot::new(
                    timestamp,
                    *state_rx.borrow(),
                    force_safe_rx.borrow().force_n,
                    *safety_rx.borrow(),
                );
                if let (Some(tx), Some(shared)) = (tx.as_ref(), shared.as_ref()) {
                    push_snapshot(tx, shared, snapshot);
                }
            }
        }
    }

    if let Some(tx) = tx.as_ref() {
        let reason = if ctx.shutdown.is_cancelled() {
            "stopped"
        } else {
            "shutdown"
        };
        send_ended(tx, last_tick, reason);
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
        let shared = DashboardShared::new(self.config.session.clone());
        let tx = match crate::server::try_start_server(
            self.config.port,
            self.config.static_dir.clone(),
            ctx.shutdown.clone(),
            shared.clone(),
        )
        .await
        {
            Ok(server) => {
                crate::preflight::print_startup_banner(server.addr, &self.config.session);
                Some(server.tx)
            }
            Err(e) => {
                error!(
                    "dashboard: failed to bind port {} — live UI disabled; control loop continues ({e})",
                    self.config.port
                );
                eprintln!();
                eprintln!(
                    "dashboard failed to start on port {}: {e}",
                    self.config.port
                );
                eprintln!("the control loop will continue without a web UI.");
                eprintln!();
                None
            }
        };

        run_bus_loop(ctx, tx, Some(shared)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helm_core::{Runtime, Timestamp, TopicBus};

    fn register_all(bus: &mut TopicBus) {
        bus.register(&topics::TICK).unwrap();
        bus.register(&topics::CART_POLE_STATE).unwrap();
        bus.register(&topics::FORCE_CMD_SAFE).unwrap();
        bus.register(&topics::SAFETY_STATUS).unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn bus_loop_pushes_tick_envelope() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let (tx, mut rx) = broadcast::channel(BROADCAST_CAPACITY);
        let shared = DashboardShared::new(crate::wire::SessionInfo::live_sim(0.01));

        let mut runtime = Runtime::new(handle.clone());
        let ctx_bus = runtime.bus();
        let shutdown = tokio_util::sync::CancellationToken::new();
        let topics = DashboardModule::new(DashboardConfig::new(0)).topics();
        let ctx = ModuleContext {
            bus: helm_core::ModuleBus::new(ctx_bus, topics),
            shutdown: shutdown.clone(),
        };

        let loop_handle =
            tokio::spawn(async move { run_bus_loop(ctx, Some(tx), Some(shared)).await });

        let run =
            tokio::spawn(async move { runtime.run_for_ticks(3, Duration::from_millis(10)).await });

        for _ in 0..3 {
            tokio::time::advance(Duration::from_millis(10)).await;
            tokio::task::yield_now().await;
        }

        run.await.unwrap().unwrap();
        shutdown.cancel();
        loop_handle.await.unwrap().unwrap();

        let json = rx.recv().await.unwrap();
        assert!(json.contains("\"type\":\"tick\""));
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

        let loop_handle = tokio::spawn(async move { run_bus_loop(ctx, None, None).await });

        let run =
            tokio::spawn(async move { runtime.run_for_ticks(2, Duration::from_millis(10)).await });

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
        let shared = DashboardShared::new(crate::wire::SessionInfo::live_sim(0.01));
        push_snapshot(
            &tx,
            &shared,
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
