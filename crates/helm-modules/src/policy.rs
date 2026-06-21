use std::path::{Path, PathBuf};

use async_trait::async_trait;

use helm_core::{
    CartPoleState, ForceCommand, Module, ModuleContext, ModuleError, ModuleTopics, module_topics,
    topics,
};

const FORCE_LIMIT: f64 = 20.0;

pub struct PolicyModule {
    model_path: PathBuf,
}

impl PolicyModule {
    pub fn new(model_path: impl Into<PathBuf>) -> Result<Self, ModuleError> {
        let model_path = model_path.into();
        if !model_path.is_file() {
            return Err(ModuleError::Failed(
                "policy",
                format!("model not found: {}", model_path.display()),
            ));
        }
        Ok(Self { model_path })
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    fn compute_force_stub(_state: CartPoleState) -> f64 {
        0.0
    }
}

#[async_trait]
impl Module for PolicyModule {
    fn name(&self) -> &'static str {
        "policy"
    }

    fn topics(&self) -> ModuleTopics {
        module_topics! {
            sub: [topics::CART_POLE_STATE],
            publish: [topics::FORCE_CMD],
        }
    }

    async fn run(&self, ctx: ModuleContext) -> Result<(), ModuleError> {
        let mut state_rx = ctx.bus.subscribe_watch(&topics::CART_POLE_STATE)?;

        let initial = *state_rx.borrow();
        ctx.bus.publish_watch(
            &topics::FORCE_CMD,
            ForceCommand {
                force_n: Self::compute_force_stub(initial).clamp(-FORCE_LIMIT, FORCE_LIMIT),
            },
        )?;

        loop {
            tokio::select! {
                _ = ctx.shutdown.cancelled() => break,
                changed = state_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let state = *state_rx.borrow_and_update();
                    let force_n = Self::compute_force_stub(state).clamp(-FORCE_LIMIT, FORCE_LIMIT);
                    ctx.bus.publish_watch(&topics::FORCE_CMD, ForceCommand { force_n })?;
                }
            }
        }

        Ok(())
    }
}
