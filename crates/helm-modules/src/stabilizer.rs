use async_trait::async_trait;

use helm_core::{
    CartPoleState, FaultConfig, FaultKind, ForceCommand, Module, ModuleContext, ModuleError,
    ModuleTopics, module_topics, topics,
};

const FORCE_LIMIT: f64 = 20.0;

const K_THETA: f64 = 120.0;
const K_THETA_DOT: f64 = 20.0;
const K_X: f64 = 1.0;
const K_X_DOT: f64 = 2.0;

pub struct StabilizerModule {
    fault: FaultConfig,
}

impl StabilizerModule {
    pub fn new() -> Self {
        Self::with_fault(FaultConfig::none())
    }

    pub fn with_fault(fault: FaultConfig) -> Self {
        Self { fault }
    }
}

impl Default for StabilizerModule {
    fn default() -> Self {
        Self::new()
    }
}

fn compute_force(state: CartPoleState) -> f64 {
    let raw = K_THETA * state.theta
        + K_THETA_DOT * state.theta_dot
        + K_X * state.x
        + K_X_DOT * state.x_dot;
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
        let mut state_rx = ctx.bus.subscribe_watch(&topics::CART_POLE_STATE)?;
        let mut tick_rx = ctx.bus.subscribe_watch(&topics::TICK)?;

        let overshoot = match self.fault.kind {
            Some(FaultKind::ForceOvershoot { at_tick, force_n }) => Some((at_tick, force_n)),
            _ => None,
        };
        let drop_after = match self.fault.kind {
            Some(FaultKind::DropCommand { after_tick }) => Some(after_tick),
            _ => None,
        };

        let initial = *state_rx.borrow();
        ctx.bus.publish_watch(
            &topics::FORCE_CMD,
            ForceCommand {
                force_n: compute_force(initial),
            },
        )?;

        loop {
            tokio::select! {
                _ = ctx.shutdown.cancelled() => break,
                changed = tick_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let _ = tick_rx.borrow_and_update();
                    let state = *state_rx.borrow();
                    let tick = tick_rx.borrow().timestamp.tick;

                    if drop_after.is_some_and(|after| tick > after) {
                        continue;
                    }

                    if overshoot.is_some_and(|(at, force_n)| tick == at) {
                        ctx.bus.publish_watch(
                            &topics::FORCE_CMD,
                            ForceCommand {
                                force_n: overshoot.unwrap().1,
                            },
                        )?;
                        continue;
                    }

                    ctx.bus.publish_watch(
                        &topics::FORCE_CMD,
                        ForceCommand {
                            force_n: compute_force(state),
                        },
                    )?;
                }
                changed = state_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let state = *state_rx.borrow_and_update();
                    let tick = tick_rx.borrow().timestamp.tick;

                    if drop_after.is_some_and(|after| tick > after) {
                        continue;
                    }

                    if overshoot.is_some_and(|(at, _)| tick == at) {
                        let force_n = overshoot.unwrap().1;
                        ctx.bus.publish_watch(
                            &topics::FORCE_CMD,
                            ForceCommand { force_n },
                        )?;
                        continue;
                    }

                    ctx.bus.publish_watch(
                        &topics::FORCE_CMD,
                        ForceCommand {
                            force_n: compute_force(state),
                        },
                    )?;
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
        assert!(compute_force(state) > 0.0);
    }
}
