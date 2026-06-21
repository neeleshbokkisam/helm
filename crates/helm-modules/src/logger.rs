use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use async_trait::async_trait;

use helm_core::{
    Module, ModuleContext, ModuleError, ModuleTopics, module_topics, topics,
};

pub struct LoggerModule {
    csv_path: Option<PathBuf>,
    print_every: u64,
}

impl LoggerModule {
    pub fn new(csv_path: Option<PathBuf>) -> Self {
        Self {
            csv_path,
            print_every: 10,
        }
    }
}

#[async_trait]
impl Module for LoggerModule {
    fn name(&self) -> &'static str {
        "logger"
    }

    fn topics(&self) -> ModuleTopics {
        module_topics! {
            sub: [topics::TICK, topics::CART_POLE_STATE, topics::FORCE_CMD],
            publish: [],
        }
    }

    async fn run(&self, ctx: ModuleContext) -> Result<(), ModuleError> {
        let mut tick_rx = ctx.bus.subscribe_watch(&topics::TICK)?;
        let state_rx = ctx.bus.subscribe_watch(&topics::CART_POLE_STATE)?;
        let force_rx = ctx.bus.subscribe_watch(&topics::FORCE_CMD)?;

        let mut writer = match &self.csv_path {
            Some(path) => {
                let file = File::create(path).map_err(|e| {
                    ModuleError::Failed("logger", e.to_string())
                })?;
                let mut w = BufWriter::new(file);
                writeln!(w, "tick,x,x_dot,theta,theta_dot,force").map_err(|e| {
                    ModuleError::Failed("logger", e.to_string())
                })?;
                Some(w)
            }
            None => None,
        };

        loop {
            tokio::select! {
                _ = ctx.shutdown.cancelled() => break,
                changed = tick_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let tick = tick_rx.borrow_and_update().timestamp;
                    let state = *state_rx.borrow();
                    let force = force_rx.borrow().force_n;

                    if let Some(w) = writer.as_mut() {
                        writeln!(
                            w,
                            "{},{},{},{},{},{}",
                            tick.tick, state.x, state.x_dot, state.theta, state.theta_dot, force
                        )
                        .map_err(|e| ModuleError::Failed("logger", e.to_string()))?;
                    }

                    if tick.tick % self.print_every == 0 {
                        println!(
                            "t={} x={:.3} th={:.3} f={:.2}",
                            tick.tick, state.x, state.theta, force
                        );
                    }
                }
            }
        }

        Ok(())
    }
}
