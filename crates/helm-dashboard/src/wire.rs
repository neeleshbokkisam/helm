use serde::Serialize;

use crate::snapshot::TickSnapshot;

pub const HISTORY_CAP: usize = 1000;

#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub mode: &'static str,
    pub backend: &'static str,
    pub dt_secs: f64,
    pub initial_theta_rad: f64,
    pub loops: bool,
}

impl SessionInfo {
    pub fn live_sim(dt_secs: f64) -> Self {
        Self {
            mode: "live",
            backend: "sim",
            dt_secs,
            initial_theta_rad: helm_core::CartPoleState::INITIAL.theta,
            loops: false,
        }
    }

    pub fn live_sim_demo(dt_secs: f64, initial_theta_rad: f64) -> Self {
        Self {
            mode: "live",
            backend: "sim",
            dt_secs,
            initial_theta_rad,
            loops: true,
        }
    }

    pub fn replay(dt_secs: f64, initial_theta_rad: f64) -> Self {
        Self {
            mode: "replay",
            backend: "replay",
            dt_secs,
            initial_theta_rad,
            loops: true,
        }
    }

    pub fn live_hardware(dt_secs: f64) -> Self {
        Self {
            mode: "live",
            backend: "hardware",
            dt_secs,
            initial_theta_rad: helm_core::CartPoleState::INITIAL.theta,
            loops: false,
        }
    }
}

impl Default for SessionInfo {
    fn default() -> Self {
        Self::live_sim(0.01)
    }
}

#[derive(Serialize)]
struct HelloWire<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    mode: &'a str,
    backend: &'a str,
    dt_secs: f64,
    initial_theta_rad: f64,
    loops: bool,
}

#[derive(Serialize)]
struct HistoryWire {
    #[serde(rename = "type")]
    kind: &'static str,
    snapshots: Vec<TickSnapshot>,
}

#[derive(Serialize)]
struct TickWire {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(flatten)]
    snapshot: TickSnapshot,
}

#[derive(Serialize)]
pub struct EndedWire {
    #[serde(rename = "type")]
    kind: &'static str,
    final_tick: u64,
    reason: String,
}

pub fn hello_json(session: &SessionInfo) -> Option<String> {
    serde_json::to_string(&HelloWire {
        kind: "hello",
        mode: session.mode,
        backend: session.backend,
        dt_secs: session.dt_secs,
        initial_theta_rad: session.initial_theta_rad,
        loops: session.loops,
    })
    .ok()
}

pub fn history_json(snapshots: &[TickSnapshot]) -> Option<String> {
    serde_json::to_string(&HistoryWire {
        kind: "history",
        snapshots: snapshots.to_vec(),
    })
    .ok()
}

pub fn tick_json(snapshot: &TickSnapshot) -> Option<String> {
    serde_json::to_string(&TickWire {
        kind: "tick",
        snapshot: snapshot.clone(),
    })
    .ok()
}

pub fn ended_json(final_tick: u64, reason: &str) -> Option<String> {
    serde_json::to_string(&EndedWire {
        kind: "ended",
        final_tick,
        reason: reason.to_string(),
    })
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use helm_core::{topics, CartPoleState, SafetyStatus, Timestamp};

    #[test]
    fn envelope_json_has_type_field() {
        let hello = hello_json(&SessionInfo::live_sim(0.01)).unwrap();
        assert!(hello.contains("\"type\":\"hello\""));

        let snap = TickSnapshot::new(
            Timestamp {
                tick: 1,
                dt_secs: 0.01,
            },
            CartPoleState::INITIAL,
            -0.1,
            SafetyStatus::INITIAL,
        );
        let tick = tick_json(&snap).unwrap();
        assert!(tick.contains("\"type\":\"tick\""));
        assert!(tick.contains("\"tick\":1"));

        let hist = history_json(&[snap]).unwrap();
        assert!(hist.contains("\"type\":\"history\""));

        let ended = ended_json(500, "duration").unwrap();
        assert!(ended.contains("\"type\":\"ended\""));
    }
}
