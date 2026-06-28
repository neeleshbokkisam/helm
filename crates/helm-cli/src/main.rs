use std::env;
use std::path::PathBuf;
use std::time::Duration;

use helm_core::{FaultConfig, FaultKind, Runtime, TopicBus, topics};
use helm_modules::{LoggerModule, SafetyConfig, SafetyModule, StabilizerModule};
use helm_sim::CartPoleModule;

#[cfg(feature = "hardware")]
use helm_hardware::{DeviceFaultConfig, HardwareConfig, HardwarePlantModule};

#[cfg(feature = "dashboard")]
use helm_dashboard::{DashboardConfig, DashboardModule};

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
    #[cfg(feature = "onnx")]
    controller: Controller,
    #[cfg(feature = "onnx")]
    model: Option<PathBuf>,
}

fn usage() {
    eprintln!("usage: helm [--seconds N] [--dt-ms N] [--csv PATH]");
    eprintln!("       [--fault force-overshoot|stale-state|dropped-cmd --fault-at N]");
    eprintln!("       [--halt-on-fault]");
    #[cfg(feature = "hardware")]
    {
        eprintln!("       [--backend sim|hardware [--spawn-fake-device | --pty-path PATH]]");
        eprintln!("       [--device-fault drop-bytes|corrupt-crc|silent|link-down --device-fault-at N]");
    }
    #[cfg(feature = "dashboard")]
    eprintln!("       [--dashboard [--dashboard-port N]]");
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
    #[cfg(feature = "onnx")]
    let mut controller = Controller::Stabilizer;
    #[cfg(feature = "onnx")]
    let mut model = None;

    let mut args = env::args().skip(1);
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
                pty_path = Some(PathBuf::from(args.next().ok_or("missing value for --pty-path")?))
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
            "--dashboard-port" => {
                dashboard_port = args
                    .next()
                    .ok_or("missing value for --dashboard-port")?
                    .parse()
                    .map_err(|_| "invalid --dashboard-port")?;
            }
            #[cfg(feature = "onnx")]
            "--controller" => {
                controller = match args.next().ok_or("missing value for --controller")?.as_str() {
                    "stabilizer" => Controller::Stabilizer,
                    "policy" => Controller::Policy,
                    other => return Err(format!("unknown controller: {other}")),
                };
            }
            #[cfg(feature = "onnx")]
            "--model" => model = Some(PathBuf::from(args.next().ok_or("missing value for --model")?)),
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
        #[cfg(feature = "onnx")]
        controller,
        #[cfg(feature = "onnx")]
        model,
    })
}

async fn run(opts: RunOptions) -> Result<(), Box<dyn std::error::Error>> {
    let (mut bus, handle) = TopicBus::new();
    bus.register(&topics::TICK)?;
    bus.register(&topics::CART_POLE_STATE)?;
    bus.register(&topics::FORCE_CMD)?;
    bus.register(&topics::FORCE_CMD_SAFE)?;
    bus.register(&topics::SAFETY_STATUS)?;

    let mut safety_config = SafetyConfig::new(opts.dt_ms);
    safety_config.halt_on_fault = opts.halt_on_fault;

    let mut runtime = Runtime::new(handle);
    #[cfg(feature = "hardware")]
    match opts.backend {
        Backend::Sim => {
            runtime.add_module(Box::new(CartPoleModule::with_fault(opts.fault)))?;
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
                return Err("--spawn-fake-device or --pty-path required for hardware backend".into());
            };
            runtime.add_module(Box::new(plant))?;
        }
    }
    #[cfg(not(feature = "hardware"))]
    runtime.add_module(Box::new(CartPoleModule::with_fault(opts.fault)))?;

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
        runtime.add_module(Box::new(DashboardModule::new(DashboardConfig::new(
            opts.dashboard_port,
        ))))?;
    }

    let ticks = opts.seconds * 1000 / opts.dt_ms.max(1);
    runtime
        .run_for_ticks(ticks, Duration::from_millis(opts.dt_ms))
        .await?;

    Ok(())
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
