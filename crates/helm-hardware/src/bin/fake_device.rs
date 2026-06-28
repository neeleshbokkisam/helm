use std::env;
use std::io::{Read, Write};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::path::PathBuf;

use helm_sim::{CartPoleParams, CartPolePhysics};
use helm_wire::{FrameParser, ParsedPayload, RspState, decode_payload};

use helm_hardware::config::{DeviceFaultConfig, DeviceFaultKind};
use helm_hardware::session::{WireSession, write_frame_sync};

#[cfg(unix)]
fn set_raw_fd(fd: i32) {
    unsafe {
        let mut termios: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut termios) != 0 {
            return;
        }
        termios.c_iflag &= !(libc::IGNBRK
            | libc::BRKINT
            | libc::PARMRK
            | libc::ISTRIP
            | libc::INLCR
            | libc::IGNCR
            | libc::ICRNL
            | libc::IXON);
        termios.c_oflag &= !libc::OPOST;
        termios.c_lflag &= !(libc::ECHO | libc::ECHONL | libc::ICANON | libc::ISIG | libc::IEXTEN);
        termios.c_cflag |= libc::CS8;
        termios.c_cc[libc::VMIN] = 1;
        termios.c_cc[libc::VTIME] = 0;
        let _ = libc::tcsetattr(fd, libc::TCSANOW, &termios);
    }
}

#[cfg(unix)]
fn set_raw_stdio() {
    set_raw_fd(std::io::stdin().as_raw_fd());
    set_raw_fd(std::io::stdout().as_raw_fd());
}

fn main() {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            usage();
            std::process::exit(1);
        }
    };

    let mut physics = CartPolePhysics::new(
        CartPoleParams::default(),
        helm_core::CartPoleState::INITIAL,
    );
    let mut parser = FrameParser::new();
    let mut wire = WireSession::new();
    let mut buf = [0u8; 256];
    let fault = opts.device_fault.kind.map(|k| (k, opts.device_fault.at_tick));

    if let Some(pty) = opts.pty {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&pty)
            .unwrap_or_else(|e| {
                eprintln!("open {}: {e}", pty.display());
                std::process::exit(1);
            });
        set_raw_fd(file.as_raw_fd());
        run_loop(&mut file, &mut physics, &mut parser, &mut wire, &mut buf, fault);
    } else {
        set_raw_stdio();
        let mut reader = unsafe { std::fs::File::from_raw_fd(libc::dup(std::io::stdin().as_raw_fd())) };
        let mut writer = unsafe { std::fs::File::from_raw_fd(libc::dup(std::io::stdout().as_raw_fd())) };
        run_stdio(
            &mut reader,
            &mut writer,
            &mut physics,
            &mut parser,
            &mut wire,
            &mut buf,
            fault,
        );
    }
}

fn run_loop(
    io: &mut (impl Read + Write),
    physics: &mut CartPolePhysics,
    parser: &mut FrameParser,
    wire: &mut WireSession,
    buf: &mut [u8; 256],
    fault: Option<(DeviceFaultKind, u32)>,
) {
    loop {
        let n = match io.read(buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                eprintln!("read: {e}");
                break;
            }
        };
        handle_bytes(io, physics, parser, wire, &buf[..n], fault);
    }
}

fn run_stdio(
    stdin: &mut impl Read,
    stdout: &mut impl Write,
    physics: &mut CartPolePhysics,
    parser: &mut FrameParser,
    wire: &mut WireSession,
    buf: &mut [u8; 256],
    fault: Option<(DeviceFaultKind, u32)>,
) {
    loop {
        let n = match stdin.read(buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                eprintln!("read: {e}");
                break;
            }
        };
        handle_bytes(stdout, physics, parser, wire, &buf[..n], fault);
    }
}

fn handle_bytes(
    writer: &mut impl Write,
    physics: &mut CartPolePhysics,
    parser: &mut FrameParser,
    wire: &mut WireSession,
    data: &[u8],
    fault: Option<(DeviceFaultKind, u32)>,
) {
    for result in parser.push_bytes(data) {
        let frame = match result {
            Ok(f) => f,
            Err(_) => continue,
        };
        let payload = match decode_payload(frame.msg_type, &frame.body) {
            Ok(ParsedPayload::CmdSetForce(cmd)) => cmd,
            _ => continue,
        };

        let force_n = payload.force_mn as f64 / 1000.0;
        let dt_secs = payload.dt_us as f64 / 1_000_000.0;
        let state = physics.step(force_n, dt_secs);
        let rsp = RspState::from_cart_pole_state(payload.tick, state);
        let out = wire.encode_state(rsp);
        let _ = write_frame_sync(writer, out, fault, payload.tick);
    }
}

struct Options {
    pty: Option<PathBuf>,
    dt_ms: u64,
    device_fault: DeviceFaultConfig,
}

fn usage() {
    eprintln!("usage: helm-fake-device [--pty PATH] [--dt-ms N]");
    eprintln!("       [--device-fault drop-bytes|corrupt-crc|silent|link-down --device-fault-at N]");
    eprintln!("  Without --pty, reads stdin and writes stdout (spawned-on-slave mode).");
}

fn parse_args() -> Result<Options, String> {
    let mut pty = None;
    let mut dt_ms = 10u64;
    let mut fault_name = None;
    let mut fault_at = None;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pty" => pty = Some(PathBuf::from(args.next().ok_or("missing --pty")?)),
            "--dt-ms" => {
                dt_ms = args
                    .next()
                    .ok_or("missing --dt-ms")?
                    .parse()
                    .map_err(|_| "invalid --dt-ms")?;
            }
            "--device-fault" => fault_name = Some(args.next().ok_or("missing --device-fault")?),
            "--device-fault-at" => {
                fault_at = Some(
                    args.next()
                        .ok_or("missing --device-fault-at")?
                        .parse()
                        .map_err(|_| "invalid --device-fault-at")?,
                );
            }
            "--help" | "-h" => {
                usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown arg: {other}")),
        }
    }

    let device_fault = match (fault_name, fault_at) {
        (None, None) => DeviceFaultConfig::none(),
        (Some(name), Some(at)) => DeviceFaultConfig::from_cli(&name, at)?,
        _ => return Err("--device-fault and --device-fault-at must be used together".into()),
    };

    Ok(Options {
        pty,
        dt_ms,
        device_fault,
    })
}
