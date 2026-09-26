use std::env;
use std::path::PathBuf;
use std::time::Duration;

use helm_core::{topics, CartPoleState, FaultConfig, FaultKind, Runtime, TopicBus};
use helm_modules::{LoggerModule, SafetyConfig, SafetyModule, StabilizerModule};
use helm_sim::CartPoleModule;

#[cfg(feature = "hardware")]
use helm_hardware::{DeviceFaultConfig, HardwareConfig, HardwarePlantModule};

#[cfg(feature = "dashboard")]
use helm_dashboard::{
    default_static_dir, validate_dashboard_preflight, DashboardConfig, DashboardModule,
    ReplayModule, SessionInfo,
};

#[cfg(feature = "onnx")]
use helm_modules::PolicyModule;

#[cfg(not(feature = "hardware"))]
type Backend = ();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "hardware")]
enum Backend {
    Sim,
    Hardware,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "onnx")]
enum Controller {
    Stabilizer,
    Policy,
}

#[derive(Clone, Copy)]
enum StressChoice {
    Off,
    Auto,
    Fixed(u32),
}

struct RunOptions {
    seconds: u64,
    dt_ms: u64,
    csv: Option<PathBuf>,
    fault: FaultConfig,
    halt_on_fault: bool,
    #[cfg(feature = "hardware")]
    backend: Backend,
    #[cfg(feature = "hardware")]
    spawn_fake_device: bool,
    #[cfg(feature = "hardware")]
    pty_path: Option<PathBuf>,
    #[cfg(feature = "hardware")]
    device_fault: DeviceFaultConfig,
    #[cfg(feature = "dashboard")]
    dashboard: bool,
    #[cfg(feature = "dashboard")]
    dashboard_port: u16,
    #[cfg(feature = "dashboard")]
    demo: bool,
    #[cfg(feature = "dashboard")]
    demo_seconds: Option<u64>,
    #[cfg(feature = "dashboard")]
    replay: Option<PathBuf>,
    #[cfg(feature = "onnx")]
    controller: Controller,
    #[cfg(feature = "onnx")]
    model: Option<PathBuf>,
    stress: StressChoice,
}

fn plant_requests_fake_serial(name: &str) -> Result<bool, String> {
    match name {
        "sim" => Ok(false),
        "fake-serial" => {
            #[cfg(feature = "hardware")]
            {
                Ok(true)
            }
            #[cfg(not(feature = "hardware"))]
            {
                Err("fake-serial requires building with --features hardware".into())
            }
        }
        other => Err(format!("unknown plant: {other}")),
    }
}

fn usage() {
    eprintln!("usage: helm [--seconds N] [--dt-ms N] [--csv PATH] [--stress [N]]");
    eprintln!(
        "       [--fault force-overshoot|stale-state|dropped-cmd|stale-command --fault-at N]"
    );
    eprintln!("       [--halt-on-fault]");
    eprintln!("       [--plant sim|fake-serial]");
    #[cfg(feature = "hardware")]
    {
        eprintln!("       [--backend sim|hardware [--spawn-fake-device | --pty-path PATH]]");
        eprintln!(
            "       [--device-fault drop-bytes|corrupt-crc|silent|link-down --device-fault-at N]"
        );
    }
    #[cfg(feature = "dashboard")]
    {
        eprintln!("       [--dashboard [--dashboard-port N]]");
        eprintln!("       [--demo [--demo-seconds N]]  (live sim + dashboard until Ctrl-C)");
        eprintln!("       [--replay PATH]  (loop recorded csv + dashboard)");
    }
    #[cfg(feature = "onnx")]
    eprintln!("       [--controller stabilizer|policy [--model PATH]]");
}

fn parse_args() -> Result<RunOptions, String> {
    let mut seconds = 5u64;
    let mut dt_ms = 10u64;
    let mut csv = None;
    let mut fault_name = None;
    let mut fault_at = None;
    let mut halt_on_fault = false;
    #[cfg(feature = "hardware")]
    let mut backend = Backend::Sim;
    #[cfg(feature = "hardware")]
    let mut spawn_fake_device = false;
    #[cfg(feature = "hardware")]
    let mut pty_path = None;
    #[cfg(feature = "hardware")]
    let mut device_fault_name = None;
    #[cfg(feature = "hardware")]
    let mut device_fault_at = None;
    #[cfg(feature = "dashboard")]
    let mut dashboard = false;
    #[cfg(feature = "dashboard")]
    let mut dashboard_port = 8080u16;
    #[cfg(feature = "dashboard")]
    let mut demo = false;
    #[cfg(feature = "dashboard")]
    let mut demo_seconds = None;
    #[cfg(feature = "dashboard")]
    let mut replay = None;
    #[cfg(feature = "onnx")]
    let mut controller = Controller::Stabilizer;
    #[cfg(feature = "onnx")]
    let mut model = None;
    let mut stress = StressChoice::Off;

    let mut args = env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seconds" => {
                seconds = args
                    .next()
                    .ok_or("missing value for --seconds")?
                    .parse()
                    .map_err(|_| "invalid --seconds")?;
            }
            "--dt-ms" => {
                dt_ms = args
                    .next()
                    .ok_or("missing value for --dt-ms")?
                    .parse()
                    .map_err(|_| "invalid --dt-ms")?;
            }
            "--csv" => csv = Some(PathBuf::from(args.next().ok_or("missing value for --csv")?)),
            "--fault" => {
                fault_name = Some(args.next().ok_or("missing value for --fault")?);
            }
            "--fault-at" => {
                fault_at = Some(
                    args.next()
                        .ok_or("missing value for --fault-at")?
                        .parse()
                        .map_err(|_| "invalid --fault-at")?,
                );
            }
            "--halt-on-fault" => halt_on_fault = true,
            "--plant" => {
                let name = args.next().ok_or("missing value for --plant")?;
                let fake = plant_requests_fake_serial(&name)?;
                #[cfg(feature = "hardware")]
                {
                    backend = if fake {
                        Backend::Hardware
                    } else {
                        Backend::Sim
                    };
                    spawn_fake_device = fake;
                }
                #[cfg(not(feature = "hardware"))]
                let _ = fake;
            }
            "--stress" => {
                if args.peek().is_some_and(|v| !v.starts_with('-')) {
                    let raw = args.next().unwrap();
                    let n = raw.parse().map_err(|_| "invalid --stress")?;
                    stress = StressChoice::Fixed(n);
                } else {
                    stress = StressChoice::Auto;
                }
            }
            #[cfg(feature = "hardware")]
            "--backend" => {
                backend = match args.next().ok_or("missing value for --backend")?.as_str() {
                    "sim" => Backend::Sim,
                    "hardware" => Backend::Hardware,
                    other => return Err(format!("unknown backend: {other}")),
                };
            }
            #[cfg(feature = "hardware")]
            "--spawn-fake-device" => spawn_fake_device = true,
            #[cfg(feature = "hardware")]
            "--pty-path" => {
                pty_path = Some(PathBuf::from(
                    args.next().ok_or("missing value for --pty-path")?,
                ))
            }
            #[cfg(feature = "hardware")]
            "--device-fault" => {
                device_fault_name = Some(args.next().ok_or("missing value for --device-fault")?);
            }
            #[cfg(feature = "hardware")]
            "--device-fault-at" => {
                device_fault_at = Some(
                    args.next()
                        .ok_or("missing value for --device-fault-at")?
                        .parse()
                        .map_err(|_| "invalid --device-fault-at")?,
                );
            }
            #[cfg(feature = "dashboard")]
            "--dashboard" => dashboard = true,
            #[cfg(feature = "dashboard")]
            "--demo" => {
                demo = true;
                dashboard = true;
            }
            #[cfg(feature = "dashboard")]
            "--demo-seconds" => {
                demo_seconds = Some(
                    args.next()
                        .ok_or("missing value for --demo-seconds")?
                        .parse()
                        .map_err(|_| "invalid --demo-seconds")?,
                );
            }
            #[cfg(feature = "dashboard")]
            "--dashboard-port" => {
                dashboard_port = args
                    .next()
                    .ok_or("missing value for --dashboard-port")?
                    .parse()
                    .map_err(|_| "invalid --dashboard-port")?;
            }
            #[cfg(feature = "dashboard")]
            "--replay" => {
                replay = Some(PathBuf::from(
                    args.next().ok_or("missing value for --replay")?,
                ));
                dashboard = true;
            }
            #[cfg(feature = "onnx")]
            "--controller" => {
                controller = match args
                    .next()
                    .ok_or("missing value for --controller")?
                    .as_str()
                {
                    "stabilizer" => Controller::Stabilizer,
                    "policy" => Controller::Policy,
                    other => return Err(format!("unknown controller: {other}")),
                };
            }
            #[cfg(feature = "onnx")]
            "--model" => {
                model = Some(PathBuf::from(
                    args.next().ok_or("missing value for --model")?,
                ))
            }
            "--help" | "-h" => {
                usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown arg: {other}")),
        }
    }

    let fault = match (fault_name, fault_at) {
        (None, None) => FaultConfig::none(),
        (Some(name), Some(at)) => FaultConfig::from_cli(&name, at)?,
        _ => return Err("--fault and --fault-at must be used together".into()),
    };

    #[cfg(feature = "hardware")]
    let device_fault = match (device_fault_name, device_fault_at) {
        (None, None) => DeviceFaultConfig::none(),
        (Some(name), Some(at)) => DeviceFaultConfig::from_cli(&name, at)?,
        _ => return Err("--device-fault and --device-fault-at must be used together".into()),
    };

    #[cfg(feature = "onnx")]
    if controller == Controller::Policy && model.is_none() {
        return Err("--model required with --controller policy".into());
    }

    if let Some(kind) = fault.kind {
        match kind {
            FaultKind::ForceOvershoot { .. } | FaultKind::DropCommand { .. } => {
                #[cfg(feature = "onnx")]
                if controller == Controller::Policy {
                    return Err(
                        "force-overshoot and dropped-cmd faults require --controller stabilizer"
                            .into(),
                    );
                }
            }
            FaultKind::StaleState { .. } => {}
        }
    }

    #[cfg(feature = "dashboard")]
    if demo && replay.is_some() {
        return Err("--demo and --replay are mutually exclusive".into());
    }

    Ok(RunOptions {
        seconds,
        dt_ms,
        csv,
        fault,
        halt_on_fault,
        #[cfg(feature = "hardware")]
        backend,
        #[cfg(feature = "hardware")]
        spawn_fake_device,
        #[cfg(feature = "hardware")]
        pty_path,
        #[cfg(feature = "hardware")]
        device_fault,
        #[cfg(feature = "dashboard")]
        dashboard,
        #[cfg(feature = "dashboard")]
        dashboard_port,
        #[cfg(feature = "dashboard")]
        demo,
        #[cfg(feature = "dashboard")]
        demo_seconds,
        #[cfg(feature = "dashboard")]
        replay,
        #[cfg(feature = "onnx")]
        controller,
        #[cfg(feature = "onnx")]
        model,
        stress,
    })
}

fn resolve_stress(choice: StressChoice) -> (u32, u32) {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);
    let threads = match choice {
        StressChoice::Off => 0,
        StressChoice::Auto => cores,
        StressChoice::Fixed(n) => n,
    };
    (threads, cores)
}

fn start_stress(threads: u32) -> StressGuard {
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    for _ in 0..threads {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let mut x = 0u64;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                std::hint::black_box(x);
            }
        });
    }
    StressGuard { stop }
}

struct StressGuard {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for StressGuard {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(feature = "dashboard")]
fn validate_dashboard(opts: &RunOptions) -> Result<(), String> {
    if !opts.dashboard && opts.replay.is_none() {
        return Ok(());
    }
    validate_dashboard_preflight(
        opts.dashboard_port,
        &default_static_dir(),
        opts.replay.as_deref(),
    )
}

async fn run(opts: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "dashboard")]
    validate_dashboard(&opts)?;
    let (mut bus, handle) = TopicBus::new();
    bus.register(&topics::TICK)?;
    bus.register(&topics::CART_POLE_STATE)?;
    bus.register(&topics::FORCE_CMD)?;
    bus.register(&topics::FORCE_CMD_SAFE)?;
    bus.register(&topics::SAFETY_STATUS)?;

    let (stress_threads, core_count) = resolve_stress(opts.stress);
    let _stress = start_stress(stress_threads);
    let dt = Duration::from_millis(opts.dt_ms);

    #[cfg(feature = "dashboard")]
    if let Some(path) = opts.replay.clone() {
        let initial_theta = ReplayModule::initial_theta(&path)?;
        let session = SessionInfo::replay(dt.as_secs_f64(), initial_theta);
        let mut runtime = Runtime::new(handle);
        runtime.set_load(stress_threads, core_count);
        runtime.add_module(Box::new(ReplayModule::new(path, opts.dt_ms)))?;
        runtime.add_module(Box::new(DashboardModule::new(
            DashboardConfig::new(opts.dashboard_port).with_session(session),
        )))?;
        let token = runtime.cancel_token();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            token.cancel();
        });
        runtime.run_until_cancelled(dt).await?;
        return Ok(());
    }

    bus.register(&topics::LOOP_STATS)?;

    let mut safety_config = SafetyConfig::new(opts.dt_ms);
    safety_config.halt_on_fault = opts.halt_on_fault;

    let mut runtime = Runtime::new(handle);
    runtime.set_load(stress_threads, core_count);
    #[cfg(feature = "hardware")]
    match opts.backend {
        Backend::Sim => {
            #[cfg(feature = "dashboard")]
            let plant = if opts.demo {
                let demo_initial = CartPoleState {
                    theta: 0.3,
                    ..CartPoleState::INITIAL
                };
                CartPoleModule::with_initial(demo_initial, opts.fault).with_demo_loop(1500)
            } else {
                CartPoleModule::with_fault(opts.fault)
            };
            #[cfg(not(feature = "dashboard"))]
            let plant = CartPoleModule::with_fault(opts.fault);
            runtime.add_module(Box::new(plant))?;
        }
        Backend::Hardware => {
            let mut hw_config = HardwareConfig::new(opts.dt_ms);
            hw_config.device_fault = opts.device_fault;
            let plant = if opts.spawn_fake_device {
                HardwarePlantModule::new(hw_config).with_spawned_device()?
            } else if let Some(path) = opts.pty_path {
                let master = tokio::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .await?;
                HardwarePlantModule::new(hw_config).with_master(master)
            } else {
                return Err(
                    "--spawn-fake-device or --pty-path required for hardware backend".into(),
                );
            };
            runtime.add_module(Box::new(plant))?;
        }
    }
    #[cfg(not(feature = "hardware"))]
    {
        let plant = {
            #[cfg(feature = "dashboard")]
            let p = if opts.demo {
                let demo_initial = CartPoleState {
                    theta: 0.3,
                    ..CartPoleState::INITIAL
                };
                CartPoleModule::with_initial(demo_initial, opts.fault).with_demo_loop(1500)
            } else {
                CartPoleModule::with_fault(opts.fault)
            };
            #[cfg(not(feature = "dashboard"))]
            let p = CartPoleModule::with_fault(opts.fault);
            p
        };
        runtime.add_module(Box::new(plant))?;
    }

    #[cfg(feature = "onnx")]
    match opts.controller {
        Controller::Stabilizer => {
            runtime.add_module(Box::new(StabilizerModule::with_fault(opts.fault)))?;
        }
        Controller::Policy => {
            let path = opts.model.expect("checked above");
            runtime.add_module(Box::new(PolicyModule::new(path)?))?;
        }
    }

    #[cfg(not(feature = "onnx"))]
    runtime.add_module(Box::new(StabilizerModule::with_fault(opts.fault)))?;

    runtime.add_module(Box::new(SafetyModule::new(safety_config)))?;
    runtime.add_module(Box::new(LoggerModule::new(opts.csv)))?;

    #[cfg(feature = "dashboard")]
    if opts.dashboard {
        let dt_secs = dt.as_secs_f64();
        #[cfg(feature = "hardware")]
        let session = if opts.spawn_fake_device {
            SessionInfo::live_fake_serial(dt_secs)
        } else if opts.backend == Backend::Hardware {
            SessionInfo::live_hardware(dt_secs)
        } else if opts.demo {
            SessionInfo::live_sim_demo(dt_secs, 0.3)
        } else {
            SessionInfo::live_sim(dt_secs)
        };
        #[cfg(not(feature = "hardware"))]
        let session = if opts.demo {
            SessionInfo::live_sim_demo(dt_secs, 0.3)
        } else {
            SessionInfo::live_sim(dt_secs)
        };

        runtime.add_module(Box::new(DashboardModule::new(
            DashboardConfig::new(opts.dashboard_port).with_session(session),
        )))?;
    }

    #[cfg(feature = "dashboard")]
    if opts.demo {
        let token = runtime.cancel_token();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            token.cancel();
        });
        if let Some(secs) = opts.demo_seconds {
            let ticks = secs * 1000 / opts.dt_ms.max(1);
            runtime.run_for_ticks(ticks, dt).await?;
        } else {
            runtime.run_until_cancelled(dt).await?;
        }
        return Ok(());
    }

    let ticks = opts.seconds * 1000 / opts.dt_ms.max(1);
    runtime.run_for_ticks(ticks, dt).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::plant_requests_fake_serial;

    #[test]
    fn fake_serial_plant_spawns_a_pty_device() {
        assert_eq!(plant_requests_fake_serial("sim").unwrap(), false);
        assert!(plant_requests_fake_serial("uart").is_err());
        #[cfg(feature = "hardware")]
        assert_eq!(plant_requests_fake_serial("fake-serial").unwrap(), true);
        #[cfg(not(feature = "hardware"))]
        assert!(plant_requests_fake_serial("fake-serial")
            .unwrap_err()
            .contains("hardware"));
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    match parse_args() {
        Ok(opts) => {
            if let Err(e) = run(opts).await {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{e}");
            usage();
            std::process::exit(1);
        }
    }
}
