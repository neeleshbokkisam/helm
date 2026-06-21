use std::env;
use std::path::PathBuf;
use std::time::Duration;

use helm_core::{Runtime, TopicBus, topics};
use helm_modules::{LoggerModule, StabilizerModule};
use helm_sim::CartPoleModule;

fn usage() {
    eprintln!("usage: helm [--seconds N] [--dt-ms N] [--csv PATH]");
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut seconds = 5u64;
    let mut dt_ms = 10u64;
    let mut csv = None;

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

    if let Err(e) = run(seconds, dt_ms, csv).await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

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
