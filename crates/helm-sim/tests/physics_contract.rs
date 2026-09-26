use std::process::Command;

use helm_core::CartPoleState;
use helm_sim::{CartPoleParams, CartPolePhysics, CONTRACT_STEPS, DEFAULT_DT_SECS};

fn contract_force(step: u64, profile: &str) -> f64 {
    match profile {
        "zero" => 0.0,
        "step-sine" => {
            if step < 100 {
                12.0
            } else {
                8.0 * (step as f64 * 0.05).sin()
            }
        }
        other => panic!("unknown profile {other}"),
    }
}

fn rust_trajectory(profile: &str) -> Vec<[f64; 4]> {
    let mut sim = CartPolePhysics::new(CartPoleParams::DEFAULT, CartPoleState::INITIAL);
    let mut out = vec![[
        sim.state().x,
        sim.state().x_dot,
        sim.state().theta,
        sim.state().theta_dot,
    ]];
    for step in 0..CONTRACT_STEPS {
        sim.step(contract_force(step, profile), DEFAULT_DT_SECS);
        out.push([
            sim.state().x,
            sim.state().x_dot,
            sim.state().theta,
            sim.state().theta_dot,
        ]);
    }
    out
}

fn python_trajectory(profile: &str) -> Vec<[f64; 4]> {
    let root = env!("CARGO_MANIFEST_DIR");
    let script = format!("{root}/../../tools/train/env.py");
    let output = Command::new("python3")
        .arg(&script)
        .arg("--dump-trajectory")
        .arg("--profile")
        .arg(profile)
        .output()
        .expect("run python3 tools/train/env.py");
    assert!(
        output.status.success(),
        "python failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse json")
}

fn assert_contract(profile: &str) {
    let rust = rust_trajectory(profile);
    let python = python_trajectory(profile);
    assert_eq!(rust.len(), python.len());
    assert_eq!(rust.len(), (CONTRACT_STEPS + 1) as usize);

    for (i, (r, p)) in rust.iter().zip(python.iter()).enumerate() {
        for (j, (&rv, &pv)) in r.iter().zip(p.iter()).enumerate() {
            let diff = (rv - pv).abs();
            assert!(
                diff < 1e-10,
                "profile {profile} step {i} component {j}: rust={rv} python={pv}"
            );
        }
    }
}

#[test]
fn rust_python_trajectory_match_zero_force() {
    assert_contract("zero");
}

#[test]
fn rust_python_trajectory_match_step_sine() {
    assert_contract("step-sine");
}
