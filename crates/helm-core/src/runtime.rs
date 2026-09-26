use std::collections::HashSet;
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::bus::BusHandle;
use crate::error::{BusError, HelmError, ModuleError};
use crate::message::{
    hz_from_elapsed, percentile, pipeline_miss, skipped_ticks, topics, LoopStats, SafetyStatus,
    Tick, Timestamp, LOOP_STATS_WINDOW,
};
use crate::module::{Module, ModuleBus, ModuleContext};

pub struct Runtime {
    bus: BusHandle,
    shutdown: CancellationToken,
    modules: Vec<Box<dyn Module>>,
    handles: Vec<JoinHandle<Result<(), ModuleError>>>,
    tick_handle: Option<JoinHandle<()>>,
    started: bool,
    publishers: HashSet<&'static str>,
    stress_threads: u32,
    core_count: u32,
}

impl Runtime {
    pub fn new(bus: BusHandle) -> Self {
        Self {
            bus,
            shutdown: CancellationToken::new(),
            modules: Vec::new(),
            handles: Vec::new(),
            tick_handle: None,
            started: false,
            publishers: HashSet::new(),
            stress_threads: 0,
            core_count: 0,
        }
    }

    pub fn set_load(&mut self, stress_threads: u32, core_count: u32) {
        self.stress_threads = stress_threads;
        self.core_count = core_count;
    }

    /// Raw bus handle without topic-declaration enforcement.
    /// For tests and external recorders only — not for use inside Module impls.
    pub fn bus(&self) -> BusHandle {
        self.bus.clone()
    }

    pub fn add_module(&mut self, module: Box<dyn Module>) -> Result<(), HelmError> {
        let topics = module.topics();
        self.bus.validate_module_topics(&topics)?;
        for name in topics.publishes {
            if !self.publishers.insert(name) {
                return Err(BusError::DuplicatePublisher(name).into());
            }
        }
        self.modules.push(module);
        Ok(())
    }

    pub async fn start(&mut self) -> Result<(), HelmError> {
        if self.started {
            return Ok(());
        }

        let modules = std::mem::take(&mut self.modules);
        for module in modules {
            let topics = module.topics();
            let ctx = ModuleContext {
                bus: ModuleBus::new(self.bus.clone(), topics),
                shutdown: self.shutdown.clone(),
            };
            let handle = tokio::spawn(async move { module.run(ctx).await });
            self.handles.push(handle);
        }

        self.started = true;
        Ok(())
    }

    pub async fn run_for_ticks(&mut self, n: u64, dt: Duration) -> Result<(), HelmError> {
        self.start().await?;

        let tick_handle = spawn_tick_loop(
            self.bus.clone(),
            self.shutdown.clone(),
            dt,
            Some(n),
            true,
            self.stress_threads,
            self.core_count,
        );
        self.tick_handle = Some(tick_handle);

        for handle in self.handles.drain(..) {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(e.into()),
                Err(e) => return Err(HelmError::Runtime(e.to_string())),
            }
        }

        if let Some(handle) = self.tick_handle.take() {
            let _ = handle.await;
        }

        Ok(())
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    pub async fn run_until_cancelled(&mut self, dt: Duration) -> Result<(), HelmError> {
        self.start().await?;

        let tick_handle = spawn_tick_loop(
            self.bus.clone(),
            self.shutdown.clone(),
            dt,
            None,
            false,
            self.stress_threads,
            self.core_count,
        );
        self.tick_handle = Some(tick_handle);

        for handle in self.handles.drain(..) {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(e.into()),
                Err(e) => return Err(HelmError::Runtime(e.to_string())),
            }
        }

        if let Some(handle) = self.tick_handle.take() {
            let _ = handle.await;
        }

        Ok(())
    }
}

fn spawn_tick_loop(
    bus: BusHandle,
    shutdown: CancellationToken,
    dt: Duration,
    max_ticks: Option<u64>,
    cancel_when_done: bool,
    stress_threads: u32,
    core_count: u32,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(dt);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut safety_rx = bus.subscribe_watch(&topics::SAFETY_STATUS).ok();
        let mut tick = 0u64;
        let mut miss_count = 0u64;
        let mut skip_count = 0u64;
        let mut jitter_window: std::collections::VecDeque<i64> =
            std::collections::VecDeque::with_capacity(LOOP_STATS_WINDOW);
        let mut first_fire: Option<tokio::time::Instant> = None;
        let mut last_fire: Option<tokio::time::Instant> = None;

        loop {
            if shutdown.is_cancelled() {
                break;
            }
            if max_ticks.is_some_and(|max| tick >= max) {
                break;
            }
            let deadline = interval.tick().await;
            if shutdown.is_cancelled() {
                break;
            }

            let fired = tokio::time::Instant::now();
            let jitter_us = signed_micros(fired, deadline);
            let period_us = match last_fire {
                Some(prev) => fired.saturating_duration_since(prev).as_micros() as u64,
                None => 0,
            };
            last_fire = Some(fired);
            if first_fire.is_none() {
                first_fire = Some(fired);
            }

            let safe_tick = safety_rx.as_ref().map(|rx| rx.borrow().tick).unwrap_or(0);
            let miss = pipeline_miss(tick, safe_tick);
            if miss {
                miss_count = miss_count.saturating_add(1);
            }
            skip_count = skip_count.saturating_add(skipped_ticks(period_us));

            tick += 1;
            let gaps = tick.saturating_sub(1);
            let hz = first_fire
                .map(|start| hz_from_elapsed(gaps, fired.saturating_duration_since(start)))
                .unwrap_or(0.0);
            let _ = bus.publish_watch(
                &topics::TICK,
                Tick {
                    timestamp: Timestamp {
                        tick,
                        dt_secs: dt.as_secs_f64(),
                    },
                },
            );
            let published_at = tokio::time::Instant::now();

            let is_last = max_ticks == Some(tick);
            let mut compute_us = 0u64;
            if let Some(rx) = safety_rx.as_mut() {
                let next_deadline = deadline + dt;
                tokio::select! {
                    biased;
                    _ = shutdown.cancelled() => break,
                    _ = wait_for_safety_tick(rx, tick) => {
                        compute_us = published_at.elapsed().as_micros() as u64;
                    }
                    _ = tokio::time::sleep_until(next_deadline), if !is_last => {}
                    _ = tokio::time::sleep(dt), if is_last => {}
                }
            }

            if jitter_window.len() == LOOP_STATS_WINDOW {
                jitter_window.pop_front();
            }
            jitter_window.push_back(jitter_us);
            let samples: Vec<i64> = jitter_window.iter().copied().collect();
            let _ = bus.publish_watch(
                &topics::LOOP_STATS,
                LoopStats {
                    tick,
                    period_us,
                    jitter_us,
                    jitter_p50_us: percentile(&samples, 50.0),
                    jitter_p99_us: percentile(&samples, 99.0),
                    jitter_max_us: samples.iter().copied().max().unwrap_or(0),
                    compute_us,
                    miss,
                    miss_count,
                    skip_count,
                    hz,
                    stress_threads,
                    core_count,
                },
            );

            if is_last || shutdown.is_cancelled() {
                break;
            }
        }

        if let Some(start) = first_fire {
            let elapsed = start.elapsed();
            let gaps = tick.saturating_sub(1);
            let hz = hz_from_elapsed(gaps, elapsed);
            eprintln!(
                "loop ticks={tick} elapsed={:.3}s hz={:.2}",
                elapsed.as_secs_f64(),
                hz
            );
        }

        if cancel_when_done {
            shutdown.cancel();
        }
    })
}

fn signed_micros(fired: tokio::time::Instant, scheduled: tokio::time::Instant) -> i64 {
    if fired >= scheduled {
        fired.saturating_duration_since(scheduled).as_micros() as i64
    } else {
        -(scheduled.saturating_duration_since(fired).as_micros() as i64)
    }
}

#[cfg(test)]
fn next_deadline(
    scheduled: tokio::time::Instant,
    fired: tokio::time::Instant,
    period: Duration,
) -> tokio::time::Instant {
    if period.is_zero() {
        return fired;
    }
    let late = fired.saturating_duration_since(scheduled);
    if late.is_zero() {
        return scheduled + period;
    }
    let steps = late.as_nanos().div_ceil(period.as_nanos()).max(1);
    let candidate = scheduled + period * (steps as u32);
    if candidate <= fired {
        candidate + period
    } else {
        candidate
    }
}

async fn wait_for_safety_tick(rx: &mut tokio::sync::watch::Receiver<SafetyStatus>, tick: u64) {
    loop {
        if rx.borrow().tick == tick {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{percentile, pipeline_miss, skipped_ticks, ModuleTopics};
    use crate::TopicBus;
    use async_trait::async_trait;

    struct DummyModule {
        topics: ModuleTopics,
    }

    #[async_trait]
    impl Module for DummyModule {
        fn name(&self) -> &'static str {
            "dummy"
        }

        fn topics(&self) -> ModuleTopics {
            self.topics.clone()
        }

        async fn run(&self, ctx: ModuleContext) -> Result<(), ModuleError> {
            ctx.shutdown.cancelled().await;
            Ok(())
        }
    }

    fn register_all(bus: &mut TopicBus) {
        bus.register(&topics::TICK).unwrap();
        bus.register(&topics::CART_POLE_STATE).unwrap();
        bus.register(&topics::FORCE_CMD).unwrap();
        bus.register(&topics::FORCE_CMD_SAFE).unwrap();
        bus.register(&topics::SAFETY_STATUS).unwrap();
    }

    #[tokio::test]
    async fn unknown_module_topic_fails_at_add() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let mut runtime = Runtime::new(handle);
        let module = DummyModule {
            topics: ModuleTopics {
                subscribes: &["bad/topic"],
                publishes: &[],
            },
        };
        assert!(runtime.add_module(Box::new(module)).is_err());
    }

    #[tokio::test]
    async fn duplicate_publisher_fails_at_add() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let mut runtime = Runtime::new(handle);
        runtime
            .add_module(Box::new(DummyModule {
                topics: crate::module_topics! {
                    sub: [topics::TICK],
                    publish: [topics::FORCE_CMD],
                },
            }))
            .unwrap();

        assert!(matches!(
            runtime.add_module(Box::new(DummyModule {
                topics: crate::module_topics! {
                    sub: [topics::CART_POLE_STATE],
                    publish: [topics::FORCE_CMD],
                },
            })),
            Err(HelmError::Bus(BusError::DuplicatePublisher("cmd/force")))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn run_for_ticks_starts_and_stops() {
        let (mut bus, handle) = TopicBus::new();
        register_all(&mut bus);

        let mut runtime = Runtime::new(handle);
        runtime
            .add_module(Box::new(DummyModule {
                topics: crate::module_topics! {
                    sub: [topics::TICK],
                    publish: [],
                },
            }))
            .unwrap();

        let run = runtime.run_for_ticks(3, Duration::from_millis(10));
        tokio::pin!(run);
        for _ in 0..3 {
            tokio::time::advance(Duration::from_millis(10)).await;
        }
        run.await.unwrap();
    }

    #[test]
    fn percentile_ranks_synthetic_jitter() {
        let samples = [1_000, 2_000, 3_000, 9_500, 10_100];
        assert_eq!(percentile(&samples, 50.0), 3_000);
        assert_eq!(percentile(&samples, 99.0), 10_100);
        assert_eq!(samples.iter().copied().max().unwrap(), 10_100);
    }

    #[test]
    fn pipeline_miss_ignores_gap_size() {
        assert!(!pipeline_miss(0, 0));
        assert!(!pipeline_miss(4, 4));
        assert!(pipeline_miss(4, 3));
    }

    #[test]
    fn slack_under_15ms_is_not_a_skipped_tick() {
        assert_eq!(skipped_ticks(10_267), 0);
        assert_eq!(skipped_ticks(14_999), 0);
        assert_eq!(skipped_ticks(15_000), 1);
        assert_eq!(skipped_ticks(20_000), 1);
        assert_eq!(skipped_ticks(25_000), 2);
    }

    #[test]
    fn skip_deadline_lands_on_the_next_future_slot() {
        let start = tokio::time::Instant::now();
        let period = Duration::from_millis(10);
        assert_eq!(next_deadline(start, start, period), start + period);
        assert_eq!(
            next_deadline(start, start + Duration::from_micros(9_500), period),
            start + period
        );
        assert_eq!(
            next_deadline(start, start + Duration::from_micros(10_100), period),
            start + period * 2
        );
    }
}
