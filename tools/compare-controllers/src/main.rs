use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use helm_core::CartPoleState;
use helm_modules::{pd_force, PolicyModule};
use helm_sim::{CartPoleParams, CartPolePhysics};

const DT: f64 = 0.01;
const STEPS: usize = 1000;
const WINDOW: usize = 100;
const ONE_DEG: f64 = std::f64::consts::PI / 180.0;
const THETAS: [f64; 6] = [0.05, -0.05, 0.1, -0.1, 0.2, -0.2];

struct Trial {
    controller: &'static str,
    theta0: f64,
    success: bool,
    settle_s: Option<f64>,
    overshoot: f64,
    effort: f64,
    peak_force: f64,
    saturation: f64,
}

struct Sample {
    t: f64,
    theta: f64,
    force: f64,
}

fn settle_index(thetas: &[f64]) -> Option<usize> {
    if thetas.len() < WINDOW {
        return None;
    }
    let mut run = 0usize;
    for (i, theta) in thetas.iter().enumerate() {
        if theta.abs() < ONE_DEG {
            run += 1;
            if run >= WINDOW {
                return Some(i);
            }
        } else {
            run = 0;
        }
    }
    None
}

fn overshoot_after_crossing(thetas: &[f64]) -> f64 {
    let mut crossed = false;
    let mut peak = 0.0f64;
    for pair in thetas.windows(2) {
        if pair[0].signum() != pair[1].signum() {
            crossed = true;
        }
        if crossed {
            peak = peak.max(pair[1].abs());
        }
    }
    if crossed {
        peak
    } else {
        thetas.iter().map(|t| t.abs()).fold(0.0, f64::max)
    }
}

fn simulate(theta0: f64, mut force_of: impl FnMut(CartPoleState) -> f64) -> (Trial, Vec<Sample>) {
    let mut sim = CartPolePhysics::new(
        CartPoleParams::DEFAULT,
        CartPoleState {
            theta: theta0,
            ..CartPoleState::INITIAL
        },
    );
    let mut samples = Vec::with_capacity(STEPS + 1);
    let mut effort = 0.0;
    let mut peak_force = 0.0f64;
    let mut saturated = 0usize;
    let mut thetas = Vec::with_capacity(STEPS + 1);

    for step in 0..=STEPS {
        let state = sim.state();
        let force = if step == STEPS { 0.0 } else { force_of(state) };
        thetas.push(state.theta);
        samples.push(Sample {
            t: step as f64 * DT,
            theta: state.theta,
            force,
        });
        if step < STEPS {
            effort += force * force * DT;
            peak_force = peak_force.max(force.abs());
            if force.abs() >= 20.0 {
                saturated += 1;
            }
            sim.step(force, DT);
        }
    }

    let settle = settle_index(&thetas);
    (
        Trial {
            controller: "",
            theta0,
            success: settle.is_some(),
            settle_s: settle.map(|i| i as f64 * DT),
            overshoot: overshoot_after_crossing(&thetas),
            effort,
            peak_force,
            saturation: saturated as f64 / STEPS as f64,
        },
        samples,
    )
}

fn main() {
    let mut metrics_path = None;
    let mut trace_path = None;
    let mut model = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models/cartpole.onnx");
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--metrics" => metrics_path = args.next().map(PathBuf::from),
            "--trace" => trace_path = args.next().map(PathBuf::from),
            "--model" => {
                model = PathBuf::from(args.next().expect("missing --model path"));
            }
            other => {
                eprintln!("unknown arg: {other}");
                std::process::exit(2);
            }
        }
    }
    let policy = PolicyModule::new(&model).unwrap_or_else(|e| {
        eprintln!("load {}: {e}", model.display());
        std::process::exit(1);
    });

    println!("model: {}", model.display());
    println!("trained linear policy, regressed onto the PD law, not the test fixture");
    println!("success = |theta| < 1 deg for 1 s within 10 s; dt = {DT} s; other state starts at 0");
    println!(
        "{:<10} {:>8} {:>7} {:>8} {:>10} {:>10} {:>8} {:>6}",
        "controller", "theta0", "success", "settle_s", "overshoot", "effort", "peak_F", "sat"
    );

    let mut trials = Vec::new();
    let mut traces: Vec<(&str, f64, Vec<Sample>)> = Vec::new();

    for theta0 in THETAS {
        let (mut pd, pd_samples) = simulate(theta0, pd_force);
        pd.controller = "pd";
        let (mut onnx, onnx_samples) =
            simulate(theta0, |state| policy.infer_force(state).expect("infer"));
        onnx.controller = "onnx";
        print_trial(&pd);
        print_trial(&onnx);
        traces.push(("pd", theta0, pd_samples));
        traces.push(("onnx", theta0, onnx_samples));
        trials.push(pd);
        trials.push(onnx);
    }

    if let Some(path) = metrics_path {
        write_metrics(&path, &trials);
    }
    if let Some(path) = trace_path {
        write_trace(&path, &traces);
    }
}

fn print_trial(trial: &Trial) {
    let settle = trial
        .settle_s
        .map(|s| format!("{s:.2}"))
        .unwrap_or_else(|| "-".into());
    println!(
        "{:<10} {:>8.3} {:>7} {:>8} {:>10.4} {:>10.3} {:>8.3} {:>6.3}",
        trial.controller,
        trial.theta0,
        if trial.success { "yes" } else { "no" },
        settle,
        trial.overshoot,
        trial.effort,
        trial.peak_force,
        trial.saturation
    );
}

fn write_metrics(path: &std::path::Path, trials: &[Trial]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create metrics dir");
    }
    let mut out = BufWriter::new(File::create(path).expect("create metrics"));
    writeln!(
        out,
        "controller,theta0,success,settle_s,overshoot_rad,effort,peak_force_n,saturation"
    )
    .unwrap();
    for trial in trials {
        let settle = trial
            .settle_s
            .map(|s| format!("{s:.4}"))
            .unwrap_or_default();
        writeln!(
            out,
            "{},{:.4},{},{},{:.6},{:.6},{:.6},{:.6}",
            trial.controller,
            trial.theta0,
            if trial.success { "yes" } else { "no" },
            settle,
            trial.overshoot,
            trial.effort,
            trial.peak_force,
            trial.saturation
        )
        .unwrap();
    }
}

fn write_trace(path: &std::path::Path, traces: &[(&str, f64, Vec<Sample>)]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create trace dir");
    }
    let mut out = BufWriter::new(File::create(path).expect("create trace"));
    writeln!(out, "controller,theta0,t,theta,force").unwrap();
    for (name, theta0, samples) in traces {
        for sample in samples {
            writeln!(
                out,
                "{name},{theta0:.4},{:.4},{:.6},{:.6}",
                sample.t, sample.theta, sample.force
            )
            .unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settle_needs_a_full_second_under_one_degree() {
        let mut thetas = vec![0.2; 150];
        assert!(settle_index(&thetas).is_none());
        for theta in thetas.iter_mut().skip(50) {
            *theta = 0.01;
        }
        assert_eq!(settle_index(&thetas), Some(149));
    }

    #[test]
    fn overshoot_is_the_peak_after_the_first_crossing() {
        let thetas = [0.2, 0.1, -0.05, -0.3, 0.02];
        let peak = overshoot_after_crossing(&thetas);
        assert!((peak - 0.3).abs() < 1e-12);
    }
}
