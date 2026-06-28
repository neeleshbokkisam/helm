use async_trait::async_trait;

use helm_core::{
    ForceCommand, Module, ModuleContext, ModuleError, ModuleTopics, SafetyFault, SafetyStatus,
    CMD_STALE_MS, SAFETY_FORWARD_CLAMP_N, SAFETY_TRIP_LIMIT_N, STATE_STALE_MS,
    module_topics, stale_ticks, topics,
};

pub struct SafetyConfig {
    pub dt_ms: u64,
    pub halt_on_fault: bool,
    pub state_stale_ms: u64,
    pub cmd_stale_ms: u64,
}

impl SafetyConfig {
    pub fn new(dt_ms: u64) -> Self {
        Self {
            dt_ms,
            halt_on_fault: false,
            state_stale_ms: STATE_STALE_MS,
            cmd_stale_ms: CMD_STALE_MS,
        }
    }

    pub fn state_stale_ticks(&self) -> u64 {
        stale_ticks(self.state_stale_ms, self.dt_ms)
    }

    pub fn cmd_stale_ticks(&self) -> u64 {
        stale_ticks(self.cmd_stale_ms, self.dt_ms)
    }
}

pub struct SafetyModule {
    config: SafetyConfig,
}

impl SafetyModule {
    pub fn new(config: SafetyConfig) -> Self {
        Self { config }
    }
}

fn forward_force(raw: f64) -> f64 {
    raw.clamp(-SAFETY_FORWARD_CLAMP_N, SAFETY_FORWARD_CLAMP_N)
}

fn safe_output(raw: f64, latched: &mut Option<SafetyFault>) -> ForceCommand {
    if latched.is_some() {
        return ForceCommand { force_n: 0.0 };
    }
    if raw.abs() > SAFETY_TRIP_LIMIT_N {
        *latched = Some(SafetyFault::ForceOutOfRange {
            requested_n: raw,
            limit_n: SAFETY_TRIP_LIMIT_N,
        });
        return ForceCommand { force_n: 0.0 };
    }
    ForceCommand {
        force_n: forward_force(raw),
    }
}

#[async_trait]
impl Module for SafetyModule {
    fn name(&self) -> &'static str {
        "safety"
    }

    fn topics(&self) -> ModuleTopics {
        module_topics! {
            sub: [topics::TICK, topics::FORCE_CMD, topics::CART_POLE_STATE],
            publish: [topics::FORCE_CMD_SAFE, topics::SAFETY_STATUS],
        }
    }

    async fn run(&self, ctx: ModuleContext) -> Result<(), ModuleError> {
        let mut tick_rx = ctx.bus.subscribe_watch(&topics::TICK)?;
        let mut force_rx = ctx.bus.subscribe_watch(&topics::FORCE_CMD)?;
        let mut state_rx = ctx.bus.subscribe_watch(&topics::CART_POLE_STATE)?;

        let mut latched: Option<SafetyFault> = None;
        let mut ticks_since_state = 0u64;
        let mut ticks_since_cmd = 0u64;
        let mut state_changed_since_cmd = false;
        let state_stale_ticks = self.config.state_stale_ticks();
        let cmd_stale_ticks = self.config.cmd_stale_ticks();

        let raw = force_rx.borrow().force_n;
        let out = safe_output(raw, &mut latched);
        ctx.bus.publish_watch(&topics::FORCE_CMD_SAFE, out)?;

        loop {
            tokio::select! {
                biased;
                _ = ctx.shutdown.cancelled() => break,
                changed = force_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let raw = force_rx.borrow_and_update().force_n;
                    ticks_since_cmd = 0;
                    state_changed_since_cmd = false;

                    if latched.is_some() {
                        ctx.bus.publish_watch(
                            &topics::FORCE_CMD_SAFE,
                            ForceCommand { force_n: 0.0 },
                        )?;
                    } else {
                        let out = safe_output(raw, &mut latched);
                        ctx.bus.publish_watch(&topics::FORCE_CMD_SAFE, out)?;
                        if latched.is_some() && self.config.halt_on_fault {
                            ctx.shutdown.cancel();
                        }
                    }

                    let tick = tick_rx.borrow().timestamp.tick;
                    ctx.bus.publish_watch(&topics::SAFETY_STATUS, SafetyStatus {
                        armed: true,
                        latched_fault: latched,
                        tick,
                    })?;
                }
                changed = state_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let _ = state_rx.borrow_and_update();
                    ticks_since_state = 0;
                    state_changed_since_cmd = true;
                }
                changed = tick_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let tick = tick_rx.borrow_and_update().timestamp.tick;
                    ticks_since_state = ticks_since_state.saturating_add(1);
                    if state_changed_since_cmd {
                        ticks_since_cmd = ticks_since_cmd.saturating_add(1);
                    }

                    if latched.is_none() {
                        if ticks_since_state >= state_stale_ticks {
                            latched = Some(SafetyFault::StateStale {
                                ticks_since_update: ticks_since_state,
                            });
                        } else if state_changed_since_cmd
                            && ticks_since_cmd >= cmd_stale_ticks
                        {
                            latched = Some(SafetyFault::CommandStale {
                                ticks_since_update: ticks_since_cmd,
                            });
                        }
                    }

                    if latched.is_some() {
                        ctx.bus.publish_watch(
                            &topics::FORCE_CMD_SAFE,
                            ForceCommand { force_n: 0.0 },
                        )?;
                        if self.config.halt_on_fault {
                            ctx.shutdown.cancel();
                        }
                    } else {
                        let raw = force_rx.borrow().force_n;
                        let out = safe_output(raw, &mut latched);
                        ctx.bus.publish_watch(&topics::FORCE_CMD_SAFE, out)?;
                        if latched.is_some() && self.config.halt_on_fault {
                            ctx.shutdown.cancel();
                        }
                    }

                    ctx.bus.publish_watch(&topics::SAFETY_STATUS, SafetyStatus {
                        armed: true,
                        latched_fault: latched,
                        tick,
                    })?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helm_core::TopicBus;

    fn register_all(bus: &mut TopicBus) {
        bus.register(&topics::TICK).unwrap();
        bus.register(&topics::CART_POLE_STATE).unwrap();
        bus.register(&topics::FORCE_CMD).unwrap();
        bus.register(&topics::FORCE_CMD_SAFE).unwrap();
        bus.register(&topics::SAFETY_STATUS).unwrap();
    }

    #[test]
    fn forward_clamps_at_18_without_fault() {
        let mut latched = None;
        let out = safe_output(20.0, &mut latched);
        assert_eq!(out.force_n, 18.0);
        assert!(latched.is_none());
    }

    #[test]
    fn trips_above_20() {
        let mut latched = None;
        let out = safe_output(20.1, &mut latched);
        assert_eq!(out.force_n, 0.0);
        assert!(matches!(
            latched,
            Some(SafetyFault::ForceOutOfRange { .. })
        ));
    }

    #[test]
    fn exactly_20_forwards_clamped_not_fault() {
        let mut latched = None;
        let out = safe_output(20.0, &mut latched);
        assert_eq!(out.force_n, 18.0);
        assert!(latched.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn publishes_safe_force_on_raw_update() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let mut runtime = helm_core::Runtime::new(handle.clone());
        runtime
            .add_module(Box::new(SafetyModule::new(SafetyConfig::new(10))))
            .unwrap();

        let run = tokio::spawn(async move {
            runtime.run_for_ticks(2, std::time::Duration::from_millis(10)).await
        });

        handle
            .publish_watch(&topics::FORCE_CMD, ForceCommand { force_n: 6.0 })
            .unwrap();
        for _ in 0..2 {
            tokio::time::advance(std::time::Duration::from_millis(10)).await;
            tokio::task::yield_now().await;
        }
        let _ = run.await;

        let safe = handle.subscribe_watch(&topics::FORCE_CMD_SAFE).unwrap();
        assert_eq!(safe.borrow().force_n, 6.0);
    }

    #[tokio::test(start_paused = true)]
    async fn overshoot_never_reaches_safe_topic() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let mut runtime = helm_core::Runtime::new(handle.clone());
        runtime
            .add_module(Box::new(SafetyModule::new(SafetyConfig::new(10))))
            .unwrap();

        let run = tokio::spawn(async move {
            runtime.run_for_ticks(2, std::time::Duration::from_millis(10)).await
        });

        handle
            .publish_watch(&topics::FORCE_CMD, ForceCommand { force_n: 999.0 })
            .unwrap();
        for _ in 0..2 {
            tokio::time::advance(std::time::Duration::from_millis(10)).await;
            tokio::task::yield_now().await;
        }
        let _ = run.await;

        let safe = handle.subscribe_watch(&topics::FORCE_CMD_SAFE).unwrap();
        assert_eq!(safe.borrow().force_n, 0.0);
        let status = handle.subscribe_watch(&topics::SAFETY_STATUS).unwrap();
        assert!(status.borrow().latched_fault.is_some());
    }
}
