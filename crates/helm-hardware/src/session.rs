use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::time::Duration;

use helm_wire::{CMD_SET_FORCE, FrameParser, RSP_STATE, RspState, CmdSetForce, encode_frame};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::config::DeviceFaultKind;

const FORCE_SCALE: f64 = 1000.0;

pub struct WireSession {
    frame_buf: [u8; 128],
}

impl WireSession {
    pub fn new() -> Self {
        Self {
            frame_buf: [0u8; 128],
        }
    }

    pub fn encode_set_force(&mut self, cmd: CmdSetForce) -> &[u8] {
        let body = cmd.encode_body();
        let n = encode_frame(CMD_SET_FORCE, &body, &mut self.frame_buf).unwrap();
        &self.frame_buf[..n]
    }

    pub fn encode_state(&mut self, rsp: RspState) -> &[u8] {
        let body = rsp.encode_body();
        let n = encode_frame(RSP_STATE, &body, &mut self.frame_buf).unwrap();
        &self.frame_buf[..n]
    }
}

fn apply_fault(
    mut out: Vec<u8>,
    fault: Option<(DeviceFaultKind, u32)>,
    tick: u32,
) -> Result<Vec<u8>, std::io::Error> {
    if let Some((kind, at)) = fault {
        if tick >= at {
            match kind {
                DeviceFaultKind::DropBytes if !out.is_empty() => {
                    out.remove(out.len() / 2);
                }
                DeviceFaultKind::CorruptCrc if out.len() >= 2 => {
                    let last = out.len() - 1;
                    out[last] ^= 0xFF;
                }
                DeviceFaultKind::Silent => return Ok(Vec::new()),
                DeviceFaultKind::LinkDown => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::NotConnected,
                        "link down",
                    ))
                }
                DeviceFaultKind::DropBytes | DeviceFaultKind::CorruptCrc => {}
            }
        }
    }
    Ok(out)
}

pub fn write_frame_sync(
    writer: &mut impl Write,
    data: &[u8],
    fault: Option<(DeviceFaultKind, u32)>,
    tick: u32,
) -> std::io::Result<()> {
    let out = apply_fault(data.to_vec(), fault, tick)?;
    if out.is_empty() {
        return Ok(());
    }
    writer.write_all(&out)?;
    writer.flush()
}

pub async fn write_frame(
    writer: &mut (impl AsyncWriteExt + Unpin),
    data: &[u8],
    fault: Option<(DeviceFaultKind, u32)>,
    tick: u32,
) -> std::io::Result<()> {
    let out = apply_fault(data.to_vec(), fault, tick)?;
    if out.is_empty() {
        return Ok(());
    }
    writer.write_all(&out).await?;
    writer.flush().await
}

fn blocking_roundtrip(
    io: &mut std::fs::File,
    frame: &[u8],
    fault: Option<(DeviceFaultKind, u32)>,
    tick: u32,
    timeout: Duration,
) -> Result<Option<RspState>, std::io::Error> {
    write_frame_sync(io, frame, fault, tick)?;

    let deadline = std::time::Instant::now() + timeout;
    let mut parser = FrameParser::new();
    let mut buf = [0u8; 256];
    let fd = io.as_raw_fd();

    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let ms = i32::try_from(remaining.as_millis()).unwrap_or(i32::MAX);
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut pfd, 1, ms) };
        if ready == 0 {
            return Ok(None);
        }
        if ready < 0 {
            return Err(std::io::Error::last_os_error());
        }

        let n = match io.read(&mut buf) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "PTY closed",
                ))
            }
            Ok(n) => n,
            Err(e) => return Err(e),
        };

        for result in parser.push_bytes(&buf[..n]) {
            if let Ok(frame) = result {
                if frame.msg_type == RSP_STATE {
                    if let Ok(rsp) = RspState::decode_body(&frame.body) {
                        if rsp.tick == tick {
                            return Ok(Some(rsp));
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

/// Write CMD_SET_FORCE and read matching RSP_STATE on the same PTY fd.
pub async fn roundtrip_set_force(
    io: &tokio::fs::File,
    wire: &mut WireSession,
    cmd: CmdSetForce,
    fault: Option<(DeviceFaultKind, u32)>,
    timeout: Duration,
) -> Result<Option<RspState>, std::io::Error> {
    let frame = wire.encode_set_force(cmd).to_vec();
    let tick = cmd.tick;
    let mut std_io = io.try_clone().await?.into_std().await;
    tokio::task::spawn_blocking(move || blocking_roundtrip(&mut std_io, &frame, fault, tick, timeout))
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?
}

pub fn spawn_reader(
    mut reader: impl AsyncReadExt + Unpin + Send + 'static,
    tx: tokio::sync::mpsc::Sender<RspState>,
    shutdown: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut parser = FrameParser::new();
        let mut buf = [0u8; 256];
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                read = reader.read(&mut buf) => {
                    match read {
                        Ok(0) => break,
                        Ok(n) => {
                            for result in parser.push_bytes(&buf[..n]) {
                                if let Ok(frame) = result {
                                    if frame.msg_type == RSP_STATE {
                                        if let Ok(rsp) = RspState::decode_body(&frame.body) {
                                            let _ = tx.send(rsp).await;
                                        }
                                    }
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    })
}

pub fn cmd_from_force(tick: u32, dt_secs: f64, force_n: f64) -> CmdSetForce {
    CmdSetForce {
        tick,
        dt_us: (dt_secs * 1_000_000.0).round() as u32,
        force_mn: (force_n * FORCE_SCALE).round() as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_from_force_scales_newtons() {
        let cmd = cmd_from_force(3, 0.01, 1.5);
        assert_eq!(cmd.tick, 3);
        assert_eq!(cmd.force_mn, 1500);
    }
}
