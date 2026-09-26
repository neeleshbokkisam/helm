use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;

use helm_core::{
    module_topics, topics, CartPoleState, ForceCommand, Module, ModuleContext, ModuleError,
    ModuleTopics, SafetyStatus, Timestamp,
};

#[derive(Clone, Debug)]
struct ReplayRow {
    tick: u64,
    state: CartPoleState,
    force_n: f64,
    force_safe_n: f64,
}

pub struct ReplayModule {
    path: PathBuf,
    dt_ms: u64,
}

impl ReplayModule {
    pub fn new(path: PathBuf, dt_ms: u64) -> Self {
        Self { path, dt_ms }
    }

    fn load_rows(path: &PathBuf) -> Result<Vec<ReplayRow>, ModuleError> {
        let file = File::open(path)
            .map_err(|e| ModuleError::Failed("replay", format!("open {}: {e}", path.display())))?;
        let reader = BufReader::new(file);
        let mut rows = Vec::new();
        for (i, line) in reader.lines().enumerate() {
            let line = line.map_err(|e| ModuleError::Failed("replay", e.to_string()))?;
            if i == 0 && line.starts_with("tick,") {
                continue;
            }
            if line.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() < 7 {
                return Err(ModuleError::Failed(
                    "replay",
                    format!("bad csv line {}: {line}", i + 1),
                ));
            }
            rows.push(ReplayRow {
                tick: parts[0].parse().map_err(|_| {
                    ModuleError::Failed("replay", format!("bad tick on line {}", i + 1))
                })?,
                state: CartPoleState {
                    x: parts[1]
                        .parse()
                        .map_err(|_| ModuleError::Failed("replay", "bad x".into()))?,
                    x_dot: parts[2]
                        .parse()
                        .map_err(|_| ModuleError::Failed("replay", "bad x_dot".into()))?,
                    theta: parts[3]
                        .parse()
                        .map_err(|_| ModuleError::Failed("replay", "bad theta".into()))?,
                    theta_dot: parts[4]
                        .parse()
                        .map_err(|_| ModuleError::Failed("replay", "bad theta_dot".into()))?,
                },
                force_n: parts[5]
                    .parse()
                    .map_err(|_| ModuleError::Failed("replay", "bad force".into()))?,
                force_safe_n: parts[6]
                    .parse()
                    .map_err(|_| ModuleError::Failed("replay", "bad force_safe".into()))?,
            });
        }
        if rows.is_empty() {
            return Err(ModuleError::Failed("replay", "csv has no data rows".into()));
        }
        Ok(rows)
    }

    pub fn initial_theta(path: &PathBuf) -> Result<f64, ModuleError> {
        Ok(Self::load_rows(path)?.first().expect("rows").state.theta)
    }
}

#[async_trait]
impl Module for ReplayModule {
    fn name(&self) -> &'static str {
        "replay"
    }

    fn topics(&self) -> ModuleTopics {
        module_topics! {
            sub: [],
            publish: [
                topics::TICK,
                topics::CART_POLE_STATE,
                topics::FORCE_CMD,
                topics::FORCE_CMD_SAFE,
                topics::SAFETY_STATUS,
            ],
        }
    }

    async fn run(&self, ctx: ModuleContext) -> Result<(), ModuleError> {
        let rows = Self::load_rows(&self.path)?;
        let dt = Duration::from_millis(self.dt_ms);
        let dt_secs = dt.as_secs_f64();

        loop {
            for row in &rows {
                if ctx.shutdown.is_cancelled() {
                    return Ok(());
                }

                let ts = Timestamp {
                    tick: row.tick,
                    dt_secs,
                };
                ctx.bus
                    .publish_watch(&topics::TICK, helm_core::Tick { timestamp: ts })?;
                ctx.bus.publish_watch(&topics::CART_POLE_STATE, row.state)?;
                ctx.bus.publish_watch(
                    &topics::FORCE_CMD,
                    ForceCommand {
                        force_n: row.force_n,
                    },
                )?;
                ctx.bus.publish_watch(
                    &topics::FORCE_CMD_SAFE,
                    ForceCommand {
                        force_n: row.force_safe_n,
                    },
                )?;
                ctx.bus.publish_watch(
                    &topics::SAFETY_STATUS,
                    SafetyStatus {
                        armed: true,
                        latched_fault: None,
                        tick: row.tick,
                    },
                )?;

                tokio::time::sleep(dt).await;
            }
        }
    }
}
