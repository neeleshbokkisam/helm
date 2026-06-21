use std::sync::Mutex;

use async_trait::async_trait;

use helm_core::{
    CartPoleState, ForceCommand, Module, ModuleContext, ModuleError, ModuleTopics, module_topics,
    topics,
};

use crate::cart_pole::{CartPoleParams, CartPolePhysics};

pub struct CartPoleModule {
    physics: Mutex<CartPolePhysics>,
}

impl CartPoleModule {
    pub fn new() -> Self {
        Self {
            physics: Mutex::new(CartPolePhysics::new(
                CartPoleParams::default(),
                CartPoleState::INITIAL,
            )),
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
            sub: [topics::TICK, topics::FORCE_CMD],
            publish: [topics::CART_POLE_STATE],
        }
    }

    async fn run(&self, ctx: ModuleContext) -> Result<(), ModuleError> {
        let mut tick_rx = ctx.bus.subscribe_watch(&topics::TICK)?;
        let mut cmd_rx = ctx.bus.subscribe_cmd(&topics::FORCE_CMD)?;

        let mut force = ForceCommand { force_n: 0.0 };

        loop {
            tokio::select! {
                _ = ctx.shutdown.cancelled() => break,
                changed = tick_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let tick = *tick_rx.borrow_and_update();

                    while let Ok(cmd) = cmd_rx.try_recv() {
                        force = cmd;
                    }

                    let mut physics = self.physics.lock().expect("physics lock");
                    let state = physics.step(force.force_n, tick.timestamp.dt_secs);
                    ctx.bus.publish_watch(&topics::CART_POLE_STATE, state)?;
                }
            }
        }

        Ok(())
    }
}
