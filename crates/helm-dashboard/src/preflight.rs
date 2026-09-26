use std::net::{SocketAddr, TcpListener};
use std::path::Path;

use crate::wire::SessionInfo;

pub fn validate_dashboard_preflight(
    port: u16,
    static_dir: &Path,
    replay: Option<&Path>,
) -> Result<(), String> {
    let index = static_dir.join("index.html");
    if !index.is_file() {
        return Err(format!(
            "dashboard UI not built (missing {}).\n\
             Build it: cd crates/helm-dashboard/frontend && npm ci && npm run build",
            index.display()
        ));
    }

    if let Some(path) = replay {
        if !path.is_file() {
            return Err(format!("replay CSV not found: {}", path.display()));
        }
    }

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    match TcpListener::bind(addr) {
        Ok(listener) => drop(listener),
        Err(e) => {
            return Err(format!(
                "port {port} is not available ({e}).\n\
                 Another helm process may still be running.\n\
                 Check: lsof -i :{port}\n\
                 Or use a different port: --dashboard-port {}",
                port + 1
            ));
        }
    }

    Ok(())
}

pub fn print_startup_banner(addr: SocketAddr, session: &SessionInfo) {
    eprintln!();
    eprintln!("dashboard ready: http://{addr}");
    eprintln!("health check:  http://{addr}/health");
    eprintln!("websocket:     ws://{addr}/ws");
    match session.mode {
        "replay" => eprintln!("mode: replay (loops automatically)"),
        "live" if session.backend == "fake-serial" => {
            eprintln!("mode: live, simulated serial device (PTY), not physical hardware");
            eprintln!("tip: keep this terminal open — closing it stops the server");
        }
        "live" if session.backend == "hardware" => {
            eprintln!("mode: live hardware");
            eprintln!("tip: keep this terminal open — closing it stops the server");
        }
        _ => {
            eprintln!("mode: live simulation");
            eprintln!("tip: keep this terminal open — closing it stops the server");
        }
    }
    eprintln!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn rejects_missing_static_dir() {
        let err =
            validate_dashboard_preflight(59999, Path::new("/nonexistent/dist"), None).unwrap_err();
        assert!(err.contains("dashboard UI not built"));
    }

    #[test]
    fn rejects_missing_replay() {
        let static_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("frontend/dist");
        if !static_dir.join("index.html").is_file() {
            return;
        }
        let err = validate_dashboard_preflight(59998, &static_dir, Some(Path::new("/no/such.csv")))
            .unwrap_err();
        assert!(err.contains("replay CSV not found"));
    }
}
