use std::time::Duration;

/// Host-side tick budget reserved for controller → safety → publish, not wire RTT.
pub const HOST_RESERVE_MS: u64 = 2;

pub fn hardware_response_timeout(dt_ms: u64) -> Duration {
    Duration::from_millis(dt_ms.saturating_sub(HOST_RESERVE_MS).max(1))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceFaultKind {
    DropBytes,
    CorruptCrc,
    Silent,
    LinkDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceFaultConfig {
    pub kind: Option<DeviceFaultKind>,
    pub at_tick: u32,
}

impl DeviceFaultConfig {
    pub fn none() -> Self {
        Self {
            kind: None,
            at_tick: 0,
        }
    }

    pub fn from_cli(name: &str, at_tick: u32) -> Result<Self, String> {
        let kind = match name {
            "drop-bytes" => DeviceFaultKind::DropBytes,
            "corrupt-crc" => DeviceFaultKind::CorruptCrc,
            "silent" => DeviceFaultKind::Silent,
            "link-down" => DeviceFaultKind::LinkDown,
            other => return Err(format!("unknown device fault: {other}")),
        };
        Ok(Self {
            kind: Some(kind),
            at_tick,
        })
    }
}

#[derive(Debug, Clone)]
pub struct HardwareConfig {
    pub dt_ms: u64,
    pub response_timeout: Duration,
    pub device_fault: DeviceFaultConfig,
}

impl HardwareConfig {
    pub fn new(dt_ms: u64) -> Self {
        Self {
            dt_ms,
            response_timeout: hardware_response_timeout(dt_ms),
            device_fault: DeviceFaultConfig::none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_timeout_is_tick_budget_not_wire_speed() {
        assert_eq!(hardware_response_timeout(10), Duration::from_millis(8));
        assert_eq!(hardware_response_timeout(1), Duration::from_millis(1));
    }
}
