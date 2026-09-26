use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

static MODELED_BAUD: AtomicU32 = AtomicU32::new(0);

use nix::pty::{openpty, Winsize};
use nix::sys::termios::{cfmakeraw, tcgetattr, tcsetattr, SetArg};
use nix::unistd::{dup2, fork, ForkResult};
use std::io::{Read, Write};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PAYLOAD: [u8; 32] = [0xA5; 32];
const ITERATIONS: usize = 1000;

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let k = (sorted.len() - 1) as f64 * (p / 100.0);
    let f = k.floor() as usize;
    let c = (k.ceil() as usize).min(sorted.len() - 1);
    if f == c {
        sorted[f]
    } else {
        sorted[f] + (sorted[c] - sorted[f]) * (k - f as f64)
    }
}

fn child_work(buf: &[u8]) {
    let mut acc = 0u64;
    for b in buf {
        acc = acc.wrapping_mul(31).wrapping_add(*b as u64);
    }
    let _ = acc;
}

fn byte_time(nbytes: usize) -> Option<Duration> {
    let baud = MODELED_BAUD.load(Ordering::Relaxed);
    if baud == 0 {
        return None;
    }
    Some(Duration::from_secs_f64(
        10.0 * nbytes as f64 / f64::from(baud),
    ))
}

fn modeled_round_trip_ms(nbytes: usize, baud: u32) -> f64 {
    if baud == 0 {
        return 0.0;
    }
    2.0 * 10.0 * nbytes as f64 / f64::from(baud) * 1000.0
}

/// macOS sleep wakes a few milliseconds late. Sleep while the remainder is
/// longer than that slack, then spin so the wait matches the byte time.
fn wait_exact(delay: Duration) {
    let deadline = Instant::now() + delay;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        let left = deadline.saturating_duration_since(now);
        if left > Duration::from_millis(8) {
            std::thread::sleep(left - Duration::from_millis(6));
        } else {
            while Instant::now() < deadline {
                std::hint::spin_loop();
            }
            return;
        }
    }
}

fn child_loop() -> ! {
    let mut stdin = unsafe { std::fs::File::from_raw_fd(0) };
    let mut stdout = unsafe { std::fs::File::from_raw_fd(1) };
    let mut buf = [0u8; 64];
    loop {
        let n = stdin.read(&mut buf).unwrap_or(0);
        if n == 0 {
            std::process::exit(0);
        }
        child_work(&buf[..n]);
        if let Some(delay) = byte_time(n) {
            wait_exact(delay);
        }
        stdout.write_all(&buf[..n]).unwrap();
        stdout.flush().unwrap();
    }
}

fn parse_args() -> (String, u32, usize) {
    let mut mode = "raw".to_string();
    let mut baud = 0u32;
    let mut iterations = ITERATIONS;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--child" => child_loop(),
            "--mode" => mode = args.next().expect("missing --mode"),
            "--baud" => {
                baud = args
                    .next()
                    .expect("missing --baud")
                    .parse()
                    .expect("invalid --baud");
            }
            "--iterations" => {
                iterations = args
                    .next()
                    .expect("missing --iterations")
                    .parse()
                    .expect("invalid --iterations");
            }
            other => panic!("unknown arg: {other}"),
        }
    }
    if mode == "modeled" && baud == 0 {
        panic!("--baud is required for --mode modeled");
    }
    (mode, baud, iterations)
}

#[tokio::main]
async fn main() {
    let (mode, baud, iterations) = parse_args();
    if mode == "modeled" {
        MODELED_BAUD.store(baud, Ordering::Relaxed);
    }

    let winsize = Winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let openpty_result = openpty(Some(&winsize), None).expect("openpty");
    let master_raw = openpty_result.master.into_raw_fd();
    let slave = openpty_result.slave;
    let mut termios = tcgetattr(&slave).expect("tcgetattr");
    cfmakeraw(&mut termios);
    tcsetattr(&slave, SetArg::TCSANOW, &termios).expect("tcsetattr");

    match unsafe { fork() }.expect("fork") {
        ForkResult::Parent { .. } => {}
        ForkResult::Child => {
            dup2(slave.as_raw_fd(), 0).expect("dup2 stdin");
            dup2(slave.as_raw_fd(), 1).expect("dup2 stdout");
            child_loop();
        }
    }

    drop(slave);
    let master = unsafe { OwnedFd::from_raw_fd(master_raw) };
    let mut master_file = tokio::fs::File::from(std::fs::File::from(master));

    tokio::time::sleep(Duration::from_millis(10)).await;

    for _ in 0..20 {
        master_file.write_all(&PAYLOAD).await.unwrap();
        master_file.flush().await.unwrap();
        let mut resp = [0u8; PAYLOAD.len()];
        master_file.read_exact(&mut resp).await.unwrap();
    }

    let mut rtts_ms = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let t0 = Instant::now();
        if let Some(delay) = byte_time(PAYLOAD.len()) {
            wait_exact(delay);
        }
        master_file.write_all(&PAYLOAD).await.unwrap();
        master_file.flush().await.unwrap();
        let mut resp = [0u8; PAYLOAD.len()];
        master_file.read_exact(&mut resp).await.unwrap();
        rtts_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
    }

    rtts_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = rtts_ms[rtts_ms.len() / 2];
    let p99 = percentile(&rtts_ms, 99.0);
    let max = rtts_ms[rtts_ms.len() - 1];
    let min = rtts_ms[0];
    let mean: f64 = rtts_ms.iter().sum::<f64>() / rtts_ms.len() as f64;

    println!("bench: tokio async parent, forked child, 32-byte echo + tiny work");
    println!("mode: {mode}");
    if mode == "modeled" {
        let modeled_ms = modeled_round_trip_ms(PAYLOAD.len(), baud);
        println!("baud: {baud}");
        println!("modeled_ms: {modeled_ms:.4}");
        println!("note: modeled 8N1 byte time on a PTY, not a UART");
    } else {
        println!("note: raw PTY, termios baud is not set");
    }
    println!("iterations: {}", rtts_ms.len());
    println!("median_ms: {median:.4}");
    println!("p99_ms: {p99:.4}");
    println!("max_ms: {max:.4}");
    println!("min_ms: {min:.4}");
    println!("mean_ms: {mean:.4}");
}
