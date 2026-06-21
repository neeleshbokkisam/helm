use std::path::{Path, PathBuf};

use async_trait::async_trait;

use helm_core::{
    CartPoleState, ForceCommand, Module, ModuleContext, ModuleError, ModuleTopics, module_topics,
    topics,
};

#[cfg(feature = "onnx")]
use crate::policy_onnx::PolicyEngine;

pub struct PolicyModule {
    model_path: PathBuf,
    #[cfg(feature = "onnx")]
    engine: PolicyEngine,
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

        #[cfg(feature = "onnx")]
        {
            let engine = PolicyEngine::load(&model_path)?;
            return Ok(Self { model_path, engine });
        }

        #[cfg(not(feature = "onnx"))]
        {
            let _ = model_path;
            Err(ModuleError::Failed(
                "policy",
                "build with --features onnx".into(),
            ))
        }
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    pub fn infer_force(&self, state: CartPoleState) -> Result<f64, ModuleError> {
        self.compute_force(state)
    }

    fn compute_force(&self, state: CartPoleState) -> Result<f64, ModuleError> {
        #[cfg(feature = "onnx")]
        {
            return self.engine.infer_force(state);
        }

        #[cfg(not(feature = "onnx"))]
        {
            let _ = state;
            Err(ModuleError::Failed(
                "policy",
                "build with --features onnx".into(),
            ))
        }
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
                force_n: self.compute_force(initial)?,
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
                    let force_n = self.compute_force(state)?;
                    ctx.bus.publish_watch(&topics::FORCE_CMD, ForceCommand { force_n })?;
                }
            }
        }

        Ok(())
    }
}
