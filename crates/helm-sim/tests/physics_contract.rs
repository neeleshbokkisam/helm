use std::process::Command;

use helm_core::CartPoleState;
use helm_sim::{CartPoleParams, CartPolePhysics, CONTRACT_STEPS, DEFAULT_DT_SECS};

fn rust_trajectory() -> Vec<[f64; 4]> {
    let mut sim = CartPolePhysics::new(CartPoleParams::DEFAULT, CartPoleState::INITIAL);
    let mut out = vec![[
        sim.state().x,
        sim.state().x_dot,
        sim.state().theta,
        sim.state().theta_dot,
    ]];
    for _ in 0..CONTRACT_STEPS {
        sim.step(0.0, DEFAULT_DT_SECS);
        out.push([
            sim.state().x,
            sim.state().x_dot,
            sim.state().theta,
            sim.state().theta_dot,
        ]);
    }
    out
}

fn python_trajectory() -> Vec<[f64; 4]> {
    let root = env!("CARGO_MANIFEST_DIR");
    let script = format!("{root}/../../tools/train/env.py");
    let output = Command::new("python3")
        .arg(&script)
        .arg("--dump-trajectory")
        .output()
        .expect("run python3 tools/train/env.py");
    assert!(
        output.status.success(),
        "python failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw: Vec<[f64; 4]> = serde_json::from_slice(&output.stdout).expect("parse json");
    raw
}

#[test]
fn rust_python_trajectory_match_zero_force() {
    let rust = rust_trajectory();
    let python = python_trajectory();
    assert_eq!(rust.len(), python.len());
    assert_eq!(rust.len(), (CONTRACT_STEPS + 1) as usize);

    for (i, (r, p)) in rust.iter().zip(python.iter()).enumerate() {
        for (j, (&rv, &pv)) in r.iter().zip(p.iter()).enumerate() {
            let diff = (rv - pv).abs();
            assert!(diff < 1e-10, "step {i} component {j}: rust={rv} python={pv}");
        }
    }
}
