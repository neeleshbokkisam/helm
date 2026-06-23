use std::path::PathBuf;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::BROADCAST_CAPACITY;

pub async fn try_start_server(
    _port: u16,
    _static_dir: PathBuf,
    _shutdown: CancellationToken,
) -> Result<broadcast::Sender<String>, String> {
    Err("not implemented".into())
}
