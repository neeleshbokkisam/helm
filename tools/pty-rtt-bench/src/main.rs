use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::{Duration, Instant};

use nix::pty::{openpty, Winsize};
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

fn child_loop() -> ! {
    let mut buf = [0u8; 64];
    loop {
        let n = std::io::stdin().read(&mut buf).unwrap_or(0);
        if n == 0 {
            std::process::exit(0);
        }
        child_work(&buf[..n]);
        std::io::stdout().write_all(&buf[..n]).unwrap();
        std::io::stdout().flush().unwrap();
    }
}

#[tokio::main]
async fn main() {
    if std::env::args().nth(1).as_deref() == Some("--child") {
        child_loop();
    }

    let winsize = Winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let openpty_result = openpty(Some(&winsize), None).expect("openpty");
    let master_raw = openpty_result.master.as_raw_fd();
    let slave = openpty_result.slave;

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

    let mut rtts_ms = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let t0 = Instant::now();
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
    println!("iterations: {}", rtts_ms.len());
    println!("median_ms: {median:.4}");
    println!("p99_ms: {p99:.4}");
    println!("max_ms: {max:.4}");
    println!("min_ms: {min:.4}");
    println!("mean_ms: {mean:.4}");
}
