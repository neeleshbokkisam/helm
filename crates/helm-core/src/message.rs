#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timestamp {
    pub tick: u64,
    pub dt_secs: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ForceCommand {
    pub force_n: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tick {
    pub timestamp: Timestamp,
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

    pub const CART_POLE_STATE: Topic<CartPoleState> = Topic::new(
        "state/cart_pole",
        TopicKind::Watch,
        CartPoleState::INITIAL,
    );

    pub const FORCE_CMD: Topic<ForceCommand> = Topic::new(
        "cmd/force",
        TopicKind::Command,
        ForceCommand { force_n: 0.0 },
    );
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
