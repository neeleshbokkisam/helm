use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ndarray::Array2;
use ort::session::Session;
use ort::value::Tensor;
use serde::Deserialize;

use helm_core::{CartPoleState, ModuleError};

#[derive(Debug, Deserialize)]
struct ModelMeta {
    observation_name: String,
    observation_shape: [usize; 2],
    action_name: String,
    action_shape: [usize; 2],
    force_limit: f64,
}

pub struct PolicyEngine {
    session: Mutex<Session>,
    meta: ModelMeta,
}

impl PolicyEngine {
    pub fn load(model_path: &Path) -> Result<Self, ModuleError> {
        let meta_path = PathBuf::from(format!("{}.json", model_path.display()));
        let meta_text = std::fs::read_to_string(&meta_path).map_err(|e| {
            ModuleError::Failed("policy", format!("read metadata: {e}"))
        })?;
        let meta: ModelMeta = serde_json::from_str(&meta_text).map_err(|e| {
            ModuleError::Failed("policy", format!("parse metadata: {e}"))
        })?;

        if meta.observation_shape != [1, 4] || meta.action_shape != [1, 1] {
            return Err(ModuleError::Failed(
                "policy",
                "unexpected tensor shapes in metadata".into(),
            ));
        }

        let mut builder = Session::builder()
            .map_err(|e| ModuleError::Failed("policy", e.to_string()))?;
        let session = builder
            .commit_from_file(model_path)
            .map_err(|e| ModuleError::Failed("policy", e.to_string()))?;

        Ok(Self {
            session: Mutex::new(session),
            meta,
        })
    }

    pub fn infer_force(&self, state: CartPoleState) -> Result<f64, ModuleError> {
        let input = Array2::from_shape_vec(
            (1, 4),
            vec![state.x as f32, state.x_dot as f32, state.theta as f32, state.theta_dot as f32],
        )
        .map_err(|e| ModuleError::Failed("policy", e.to_string()))?;

        let tensor = Tensor::from_array(input)
            .map_err(|e| ModuleError::Failed("policy", e.to_string()))?;

        let mut session = self.session.lock().expect("policy session lock");
        let outputs = session
            .run(ort::inputs![self.meta.observation_name.as_str() => tensor])
            .map_err(|e| ModuleError::Failed("policy", e.to_string()))?;

        let output = outputs
            .get(self.meta.action_name.as_str())
            .ok_or_else(|| ModuleError::Failed("policy", "missing action output".into()))?;

        let (shape, data) = output
            .try_extract_tensor::<f32>()
            .map_err(|e| ModuleError::Failed("policy", e.to_string()))?;

        if shape.len() != 2 || shape[0] != 1 || shape[1] != 1 {
            return Err(ModuleError::Failed(
                "policy",
                format!("bad action shape: {shape:?}"),
            ));
        }

        let force = data[0] as f64;
        Ok(force.clamp(-self.meta.force_limit, self.meta.force_limit))
    }
}
