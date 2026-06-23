use helm_core::{CartPoleState, SafetyStatus, Timestamp};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TickSnapshot {
    pub tick: u64,
    pub dt_secs: f64,
    pub state: CartPoleState,
    pub force_safe_n: f64,
    pub safety: SafetyStatus,
}

impl TickSnapshot {
    pub fn new(
        timestamp: Timestamp,
        state: CartPoleState,
        force_safe_n: f64,
        safety: SafetyStatus,
    ) -> Self {
        Self {
            tick: timestamp.tick,
            dt_secs: timestamp.dt_secs,
            state,
            force_safe_n,
            safety,
        }
    }

    pub fn to_json(&self) -> Option<String> {
        serde_json::to_string(self).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helm_core::topics;

    #[test]
    fn snapshot_json_roundtrip_fields() {
        let snap = TickSnapshot::new(
            topics::TICK.seed.timestamp,
            topics::CART_POLE_STATE.seed,
            1.5,
            topics::SAFETY_STATUS.seed,
        );
        let json = snap.to_json().unwrap();
        assert!(json.contains("\"force_safe_n\":1.5"));
        assert!(json.contains("\"theta\":0.05"));
    }
}
