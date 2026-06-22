use std::sync::Mutex;

use async_trait::async_trait;

use helm_core::{
    CartPoleState, FaultConfig, FaultKind, Module, ModuleContext, ModuleError, ModuleTopics,
    module_topics, topics,
};

use crate::cart_pole::{CartPoleParams, CartPolePhysics};

pub struct CartPoleModule {
    physics: Mutex<CartPolePhysics>,
    fault: FaultConfig,
}

impl CartPoleModule {
    pub fn new() -> Self {
        Self::with_fault(FaultConfig::none())
    }

    pub fn with_fault(fault: FaultConfig) -> Self {
        Self {
            physics: Mutex::new(CartPolePhysics::new(
                CartPoleParams::default(),
                CartPoleState::INITIAL,
            )),
            fault,
        }
    }
}

impl Default for CartPoleModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Module for CartPoleModule {
    fn name(&self) -> &'static str {
        "cart_pole_sim"
    }

    fn topics(&self) -> ModuleTopics {
        module_topics! {
            sub: [topics::TICK, topics::FORCE_CMD_SAFE],
            publish: [topics::CART_POLE_STATE],
        }
    }

    async fn run(&self, ctx: ModuleContext) -> Result<(), ModuleError> {
        let mut tick_rx = ctx.bus.subscribe_watch(&topics::TICK)?;
        let force_rx = ctx.bus.subscribe_watch(&topics::FORCE_CMD_SAFE)?;

        let stale_after = match self.fault.kind {
            Some(FaultKind::StaleState { after_tick }) => Some(after_tick),
            _ => None,
        };

        loop {
            tokio::select! {
                _ = ctx.shutdown.cancelled() => break,
                changed = tick_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let tick = *tick_rx.borrow_and_update();
                    let force = *force_rx.borrow();

                    let mut physics = self.physics.lock().expect("physics lock");
                    let state = physics.step(force.force_n, tick.timestamp.dt_secs);

                    if stale_after.is_some_and(|after| tick.timestamp.tick > after) {
                        continue;
                    }

                    ctx.bus.publish_watch(&topics::CART_POLE_STATE, state)?;
                }
            }
        }

        Ok(())
    }
}
