use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Timestamp {
    pub tick: u64,
    pub dt_secs: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CartPoleState {
    pub x: f64,
    pub x_dot: f64,
    pub theta: f64,
    pub theta_dot: f64,
}

impl CartPoleState {
    pub const INITIAL: CartPoleState = CartPoleState {
        x: 0.0,
        x_dot: 0.0,
        theta: 0.05,
        theta_dot: 0.0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct ForceCommand {
    pub force_n: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum SafetyFault {
    ForceOutOfRange { requested_n: f64, limit_n: f64 },
    StateStale { ticks_since_update: u64 },
    CommandStale { ticks_since_update: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct SafetyStatus {
    pub armed: bool,
    pub latched_fault: Option<SafetyFault>,
    pub tick: u64,
}

impl SafetyStatus {
    pub const INITIAL: SafetyStatus = SafetyStatus {
        armed: true,
        latched_fault: None,
        tick: 0,
    };
}

/// max magnitude forwarded to sim (silent clamp, no fault)
pub const SAFETY_FORWARD_CLAMP_N: f64 = 18.0;
/// latch ForceOutOfRange when |intent| exceeds this
pub const SAFETY_TRIP_LIMIT_N: f64 = 20.0;
pub const STATE_STALE_MS: u64 = 50;
pub const CMD_STALE_MS: u64 = 50;

pub fn stale_ticks(stale_ms: u64, dt_ms: u64) -> u64 {
    let dt = dt_ms.max(1);
    stale_ms.div_ceil(dt)
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Tick {
    pub timestamp: Timestamp,
}

pub const LOOP_STATS_WINDOW: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct LoopStats {
    pub tick: u64,
    pub period_us: u64,
    pub jitter_us: i64,
    pub jitter_p50_us: i64,
    pub jitter_p99_us: i64,
    pub jitter_max_us: i64,
    pub compute_us: u64,
    pub miss: bool,
    pub miss_count: u64,
    pub hz: f64,
    pub stress_threads: u32,
    pub core_count: u32,
}

impl LoopStats {
    pub const INITIAL: Self = Self {
        tick: 0,
        period_us: 0,
        jitter_us: 0,
        jitter_p50_us: 0,
        jitter_p99_us: 0,
        jitter_max_us: 0,
        compute_us: 0,
        miss: false,
        miss_count: 0,
        hz: 0.0,
        stress_threads: 0,
        core_count: 0,
    };
}

impl Default for LoopStats {
    fn default() -> Self {
        Self::INITIAL
    }
}

/// Safe force for `previous_tick` was not on the bus when the next tick fired.
pub fn pipeline_miss(previous_tick: u64, safe_tick: u64) -> bool {
    previous_tick > 0 && safe_tick != previous_tick
}

pub fn percentile(samples: &[i64], pct: f64) -> i64 {
    if samples.is_empty() {
        return 0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let last = sorted.len() - 1;
    let idx = ((pct / 100.0) * last as f64).round() as usize;
    sorted[idx.min(last)]
}

pub fn hz_from_period_us(period_us: u64) -> f64 {
    if period_us == 0 {
        0.0
    } else {
        1_000_000.0 / period_us as f64
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopicKind {
    Watch,
    Command,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ModuleTopics {
    pub subscribes: &'static [&'static str],
    pub publishes: &'static [&'static str],
}

#[derive(Clone, Copy)]
pub struct Topic<T: Clone + Copy + Send + Sync + 'static> {
    pub name: &'static str,
    pub kind: TopicKind,
    pub seed: T,
}

impl<T: Clone + Copy + Send + Sync + 'static> Topic<T> {
    pub const fn new(name: &'static str, kind: TopicKind, seed: T) -> Self {
        Self { name, kind, seed }
    }
}

pub mod topics {
    use super::*;

    pub const TICK: Topic<Tick> = Topic::new(
        "clock/tick",
        TopicKind::Watch,
        Tick {
            timestamp: Timestamp {
                tick: 0,
                dt_secs: 0.01,
            },
        },
    );

    pub const CART_POLE_STATE: Topic<CartPoleState> =
        Topic::new("state/cart_pole", TopicKind::Watch, CartPoleState::INITIAL);

    pub const FORCE_CMD: Topic<ForceCommand> =
        Topic::new("cmd/force", TopicKind::Watch, ForceCommand { force_n: 0.0 });

    pub const FORCE_CMD_SAFE: Topic<ForceCommand> = Topic::new(
        "cmd/force_safe",
        TopicKind::Watch,
        ForceCommand { force_n: 0.0 },
    );

    pub const SAFETY_STATUS: Topic<SafetyStatus> =
        Topic::new("state/safety", TopicKind::Watch, SafetyStatus::INITIAL);

    pub const LOOP_STATS: Topic<LoopStats> =
        Topic::new("clock/loop_stats", TopicKind::Watch, LoopStats::INITIAL);
}

#[macro_export]
macro_rules! module_topics {
    (sub: [$($sub:expr),* $(,)?], publish: [] $(,)?) => {
        $crate::ModuleTopics {
            subscribes: &[$( $sub.name ),*],
            publishes: &[],
        }
    };
    (sub: [$($sub:expr),* $(,)?], publish: [$($pub:expr),+ $(,)?] $(,)?) => {
        $crate::ModuleTopics {
            subscribes: &[$( $sub.name ),*],
            publishes: &[$( $pub.name ),*],
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cart_pole_state_serializes() {
        let json = serde_json::to_string(&CartPoleState::INITIAL).unwrap();
        assert!(json.contains("\"theta\":0.05"));
    }
}
