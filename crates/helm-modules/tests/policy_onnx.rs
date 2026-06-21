#![cfg(feature = "onnx")]

use std::path::PathBuf;

use helm_core::CartPoleState;

use helm_modules::PolicyModule;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn loads_fixture_model() {
    PolicyModule::new(fixture("cartpole_test.onnx")).expect("load policy");
}

#[test]
fn linear_fixture_opposes_tilt() {
    let policy = PolicyModule::new(fixture("cartpole_test.onnx")).expect("load policy");
    let state = CartPoleState {
        theta: 0.1,
        ..CartPoleState::INITIAL
    };
    let force = policy.infer_force(state).expect("infer");
    assert!((force - 12.0).abs() < 1e-3);
}

#[test]
fn rejects_missing_metadata() {
    let dir = tempfile::tempdir().expect("tempdir");
    let model = dir.path().join("model.onnx");
    std::fs::copy(fixture("cartpole_test.onnx"), &model).expect("copy onnx");

    let err = match PolicyModule::new(model) {
        Err(e) => e,
        Ok(_) => panic!("expected metadata error"),
    };
    assert!(err.to_string().contains("metadata"));
}
