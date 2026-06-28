use std::io;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use nix::pty::{openpty, Winsize};

use crate::config::DeviceFaultConfig;

pub struct PtyEndpoints {
    pub io: tokio::fs::File,
    pub slave_path: PathBuf,
}

pub struct FakeDeviceSpawn {
    pub child: Child,
}

fn slave_path(master: &impl AsRawFd) -> io::Result<PathBuf> {
    let path = unsafe {
        let ptr = libc::ptsname(master.as_raw_fd());
        if ptr.is_null() {
            return Err(io::Error::last_os_error());
        }
        std::ffi::CStr::from_ptr(ptr)
            .to_string_lossy()
            .into_owned()
    };
    Ok(PathBuf::from(path))
}

fn set_raw_fd(fd: i32) -> io::Result<()> {
    unsafe {
        let mut termios: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut termios) != 0 {
            return Err(io::Error::last_os_error());
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
        if libc::tcsetattr(fd, libc::TCSANOW, &termios) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn open_tokio_pty(raw: i32) -> io::Result<tokio::fs::File> {
    set_raw_fd(raw)?;
    Ok(tokio::fs::File::from_std(unsafe {
        std::fs::File::from_raw_fd(raw)
    }))
}

pub fn connect_fake_device(
    dt_ms: u64,
    fault: DeviceFaultConfig,
) -> io::Result<(PtyEndpoints, FakeDeviceSpawn)> {
    let pair = openpty(
        Some(&Winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }),
        None,
    )
    .map_err(|e| io::Error::other(e))?;

    unsafe {
        if libc::grantpt(pair.master.as_raw_fd()) != 0 {
            return Err(io::Error::last_os_error());
        }
        if libc::unlockpt(pair.master.as_raw_fd()) != 0 {
            return Err(io::Error::last_os_error());
        }
    }

    let slave_path = slave_path(&pair.master)?;
    let master = pair.master;
    let slave_fd = pair.slave.into_raw_fd();
    let stdin_fd = unsafe { libc::dup(slave_fd) };
    if stdin_fd < 0 {
        unsafe {
            libc::close(slave_fd);
        }
        return Err(io::Error::last_os_error());
    }
    let child = spawn_fake_device_stdio(stdin_fd, slave_fd, dt_ms, fault)?;
    std::thread::sleep(Duration::from_millis(150));

    let raw = master.as_raw_fd();
    let io_fd = unsafe { libc::dup(raw) };
    if io_fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let io = open_tokio_pty(io_fd)?;
    drop(master);

    Ok((
        PtyEndpoints {
            io,
            slave_path,
        },
        child,
    ))
}

pub fn open_pty_endpoints() -> io::Result<PtyEndpoints> {
    connect_fake_device(10, DeviceFaultConfig::none()).map(|(e, _c)| e)
}

fn fake_device_bin() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_helm-fake-device") {
        return PathBuf::from(path);
    }
    if let Ok(path) = std::env::var("HELM_FAKE_DEVICE_BIN") {
        return PathBuf::from(path);
    }
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let target = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    target.join(profile).join("helm-fake-device")
}

fn push_fault_args(cmd: &mut Command, fault: DeviceFaultConfig) {
    if let Some(kind) = fault.kind {
        let name = match kind {
            crate::config::DeviceFaultKind::DropBytes => "drop-bytes",
            crate::config::DeviceFaultKind::CorruptCrc => "corrupt-crc",
            crate::config::DeviceFaultKind::Silent => "silent",
            crate::config::DeviceFaultKind::LinkDown => "link-down",
        };
        cmd.arg("--device-fault")
            .arg(name)
            .arg("--device-fault-at")
            .arg(fault.at_tick.to_string());
    }
}

/// Spawn fake device with stdin/stdout wired to a PTY slave (matches Python RTT harness).
pub fn spawn_fake_device_stdio(
    stdin_fd: i32,
    stdout_fd: i32,
    dt_ms: u64,
    fault: DeviceFaultConfig,
) -> io::Result<FakeDeviceSpawn> {
    let mut cmd = Command::new(fake_device_bin());
    cmd.arg("--dt-ms").arg(dt_ms.to_string());
    push_fault_args(&mut cmd, fault);
    cmd.stdin(unsafe { Stdio::from_raw_fd(stdin_fd) })
        .stdout(unsafe { Stdio::from_raw_fd(stdout_fd) })
        .stderr(Stdio::null());
    Ok(FakeDeviceSpawn {
        child: cmd.spawn()?,
    })
}

/// Spawn fake device that opens a PTY slave path (manual / CLI use).
pub fn spawn_fake_device(
    slave_path: &Path,
    dt_ms: u64,
    fault: DeviceFaultConfig,
) -> io::Result<FakeDeviceSpawn> {
    let mut cmd = Command::new(fake_device_bin());
    cmd.arg("--pty")
        .arg(slave_path)
        .arg("--dt-ms")
        .arg(dt_ms.to_string());
    push_fault_args(&mut cmd, fault);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    Ok(FakeDeviceSpawn {
        child: cmd.spawn()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_fake_device_returns_slave_path() {
        let (endpoints, _child) = connect_fake_device(10, DeviceFaultConfig::none()).unwrap();
        assert!(endpoints.slave_path.starts_with("/dev/"));
    }
}
