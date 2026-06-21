use async_trait::async_trait;

use helm_core::{
    CartPoleState, ForceCommand, Module, ModuleContext, ModuleError, ModuleTopics, module_topics,
    topics,
};

const FORCE_LIMIT: f64 = 20.0;

const K_THETA: f64 = 120.0;
const K_THETA_DOT: f64 = 20.0;
const K_X: f64 = 1.0;
const K_X_DOT: f64 = 2.0;

pub struct StabilizerModule;

fn compute_force(state: CartPoleState) -> f64 {
    let raw = -K_THETA * state.theta
        - K_THETA_DOT * state.theta_dot
        - K_X * state.x
        - K_X_DOT * state.x_dot;
    raw.clamp(-FORCE_LIMIT, FORCE_LIMIT)
}

#[async_trait]
impl Module for StabilizerModule {
    fn name(&self) -> &'static str {
        "stabilizer"
    }

    fn topics(&self) -> ModuleTopics {
        module_topics! {
            sub: [topics::TICK, topics::CART_POLE_STATE],
            publish: [topics::FORCE_CMD],
        }
    }

    async fn run(&self, ctx: ModuleContext) -> Result<(), ModuleError> {
        let mut tick_rx = ctx.bus.subscribe_watch(&topics::TICK)?;
        let mut state_rx = ctx.bus.subscribe_watch(&topics::CART_POLE_STATE)?;

        loop {
            tokio::select! {
                _ = ctx.shutdown.cancelled() => break,
                changed = tick_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let _tick = *tick_rx.borrow_and_update();
                    let state = *state_rx.borrow();
                    let force_n = compute_force(state);
                    ctx.bus.publish_cmd(&topics::FORCE_CMD, ForceCommand { force_n })?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_opposes_tilt() {
        let state = CartPoleState {
            theta: 0.1,
            ..CartPoleState::INITIAL
        };
        assert!(compute_force(state) < 0.0);
    }
}
