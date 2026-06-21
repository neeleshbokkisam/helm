use std::env;
use std::path::PathBuf;
use std::time::Duration;

use helm_core::{Runtime, TopicBus, topics};
use helm_modules::{LoggerModule, StabilizerModule};
use helm_sim::CartPoleModule;

#[cfg(feature = "onnx")]
use helm_modules::PolicyModule;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "onnx")]
enum Controller {
    Stabilizer,
    Policy,
}

fn usage() {
    eprintln!("usage: helm [--seconds N] [--dt-ms N] [--csv PATH]");
    #[cfg(feature = "onnx")]
    eprintln!("       helm --controller stabilizer|policy [--model PATH] ...");
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut seconds = 5u64;
    let mut dt_ms = 10u64;
    let mut csv = None;
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
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| {
                        usage();
                        std::process::exit(1);
                    });
            }
            "--dt-ms" => {
                dt_ms = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| {
                        usage();
                        std::process::exit(1);
                    });
            }
            "--csv" => {
                csv = args.next().map(PathBuf::from);
            }
            #[cfg(feature = "onnx")]
            "--controller" => {
                let value = args.next().unwrap_or_else(|| {
                    usage();
                    std::process::exit(1);
                });
                controller = match value.as_str() {
                    "stabilizer" => Controller::Stabilizer,
                    "policy" => Controller::Policy,
                    other => {
                        eprintln!("unknown controller: {other}");
                        usage();
                        std::process::exit(1);
                    }
                };
            }
            #[cfg(feature = "onnx")]
            "--model" => {
                model = Some(args.next().map(PathBuf::from).unwrap_or_else(|| {
                    usage();
                    std::process::exit(1);
                }));
            }
            "--help" | "-h" => {
                usage();
                return;
            }
            other => {
                eprintln!("unknown arg: {other}");
                usage();
                std::process::exit(1);
            }
        }
    }

    #[cfg(feature = "onnx")]
    {
        if controller == Controller::Policy && model.is_none() {
            eprintln!("--model required with --controller policy");
            std::process::exit(1);
        }
        if let Err(e) = run(seconds, dt_ms, csv, controller, model).await {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }

    #[cfg(not(feature = "onnx"))]
    if let Err(e) = run(seconds, dt_ms, csv).await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(feature = "onnx")]
async fn run(
    seconds: u64,
    dt_ms: u64,
    csv: Option<PathBuf>,
    controller: Controller,
    model: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut bus, handle) = TopicBus::new();
    bus.register(&topics::TICK)?;
    bus.register(&topics::CART_POLE_STATE)?;
    bus.register(&topics::FORCE_CMD)?;

    let mut runtime = Runtime::new(handle);
    runtime.add_module(Box::new(CartPoleModule::new()))?;

    match controller {
        Controller::Stabilizer => {
            runtime.add_module(Box::new(StabilizerModule))?;
        }
        Controller::Policy => {
            let path = model.expect("model checked above");
            runtime.add_module(Box::new(PolicyModule::new(path)?))?;
        }
    }

    runtime.add_module(Box::new(LoggerModule::new(csv)))?;

    let ticks = seconds * 1000 / dt_ms.max(1);
    runtime
        .run_for_ticks(ticks, Duration::from_millis(dt_ms))
        .await?;

    Ok(())
}

#[cfg(not(feature = "onnx"))]
async fn run(
    seconds: u64,
    dt_ms: u64,
    csv: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut bus, handle) = TopicBus::new();
    bus.register(&topics::TICK)?;
    bus.register(&topics::CART_POLE_STATE)?;
    bus.register(&topics::FORCE_CMD)?;

    let mut runtime = Runtime::new(handle);
    runtime.add_module(Box::new(CartPoleModule::new()))?;
    runtime.add_module(Box::new(StabilizerModule))?;
    runtime.add_module(Box::new(LoggerModule::new(csv)))?;

    let ticks = seconds * 1000 / dt_ms.max(1);
    runtime
        .run_for_ticks(ticks, Duration::from_millis(dt_ms))
        .await?;

    Ok(())
}
