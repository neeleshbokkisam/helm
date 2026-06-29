use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use helm_core::{
    Module, ModuleContext, ModuleError, ModuleTopics, Timestamp, module_topics, topics,
};

use crate::config::HardwareConfig;
use crate::session::{WireSession, cmd_from_force, roundtrip_set_force};
use crate::transport::connect_fake_device;

pub struct HardwarePlantModule {
    config: HardwareConfig,
    io: Option<tokio::fs::File>,
    child: Option<Arc<Mutex<std::process::Child>>>,
}

impl HardwarePlantModule {
    pub fn new(config: HardwareConfig) -> Self {
        Self {
            config,
            io: None,
            child: None,
        }
    }

    pub fn with_spawned_device(mut self) -> Result<Self, ModuleError> {
        let (endpoints, spawned) = connect_fake_device(self.config.dt_ms, self.config.device_fault)
            .map_err(|e| ModuleError::Failed("cart_pole_hardware", e.to_string()))?;
        self.io = Some(endpoints.io);
        self.child = Some(Arc::new(Mutex::new(spawned.child)));
        Ok(self)
    }

    pub fn with_master(mut self, master: tokio::fs::File) -> Self {
        self.io = Some(master);
        self
    }
}

#[async_trait]
impl Module for HardwarePlantModule {
    fn name(&self) -> &'static str {
        "cart_pole_hardware"
    }

    fn topics(&self) -> ModuleTopics {
        module_topics! {
            sub: [topics::TICK, topics::FORCE_CMD_SAFE],
            publish: [topics::CART_POLE_STATE],
        }
    }

    async fn run(&self, ctx: ModuleContext) -> Result<(), ModuleError> {
        let io_src = self
            .io
            .as_ref()
            .ok_or_else(|| ModuleError::Failed("cart_pole_hardware", "missing PTY".into()))?;
        let io = Arc::new(Mutex::new(
            io_src
                .try_clone()
                .await
                .map_err(|e| ModuleError::Failed("cart_pole_hardware", e.to_string()))?,
        ));

        let mut tick_rx = ctx.bus.subscribe_watch(&topics::TICK)?;
        let force_rx = ctx.bus.subscribe_watch(&topics::FORCE_CMD_SAFE)?;
        let mut wire = WireSession::new();
        let timeout = self.config.response_timeout;
        let fault = self
            .config
            .device_fault
            .kind
            .map(|k| (k, self.config.device_fault.at_tick));
        let mut link_up = true;
        let mut last_tick = 0u64;

        'run: loop {
            tokio::select! {
                _ = ctx.shutdown.cancelled() => break 'run,
                changed = tick_rx.changed() => {
                    if changed.is_err() {
                        break 'run;
                    }
                    let latest = tick_rx.borrow_and_update().timestamp;
                    while last_tick < latest.tick {
                        if !link_up {
                            break;
                        }
                        last_tick += 1;
                        let tick = Timestamp {
                            tick: last_tick,
                            dt_secs: latest.dt_secs,
                        };
                        let force = *force_rx.borrow();
                        let cmd = cmd_from_force(
                            tick.tick as u32,
                            tick.dt_secs,
                            force.force_n,
                        );
                        let guard = io.lock().await;
                        match roundtrip_set_force(
                            &*guard,
                            &mut wire,
                            cmd,
                            fault,
                            timeout,
                        )
                        .await
                        {
                            Ok(Some(rsp)) => {
                                let state = rsp.to_cart_pole_state();
                                // watch skips changed() on bit-identical payloads; fixed-point
                                // quantization makes repeated-value ticks common here.
                                if ctx
                                    .bus
                                    .publish_watch(&topics::CART_POLE_STATE, state)
                                    .is_err()
                                {
                                    break 'run;
                                }
                            }
                            Ok(None) => {
                                tracing::warn!(
                                    tick = tick.tick,
                                    "hardware plant response timeout"
                                );
                            }
                            Err(e) => {
                                link_up = false;
                                tracing::warn!("hardware plant IO failed: {e}; link down");
                            }
                        }
                    }
                }
            }
        }

        if let Some(child) = &self.child {
            let _ = child.lock().await.kill();
        }
        Ok(())
    }
}
