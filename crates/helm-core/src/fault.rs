#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FaultKind {
    ForceOvershoot { at_tick: u64, force_n: f64 },
    StaleState { after_tick: u64 },
    DropCommand { after_tick: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FaultConfig {
    pub kind: Option<FaultKind>,
}

impl FaultConfig {
    pub fn none() -> Self {
        Self { kind: None }
    }

    pub fn from_cli(name: &str, at: u64) -> Result<Self, String> {
        let kind = match name {
            "force-overshoot" => FaultKind::ForceOvershoot {
                at_tick: at,
                force_n: 999.0,
            },
            "stale-state" => FaultKind::StaleState { after_tick: at },
            "dropped-cmd" => FaultKind::DropCommand { after_tick: at },
            other => return Err(format!("unknown fault: {other}")),
        };
        Ok(Self { kind: Some(kind) })
    }
}

#[cfg(test)]
mod tests {
    use crate::stale_ticks;

    #[test]
    fn stale_ticks_scales_with_dt() {
        assert_eq!(stale_ticks(50, 10), 5);
        assert_eq!(stale_ticks(50, 20), 3);
        assert_eq!(stale_ticks(50, 7), 8);
    }
}
