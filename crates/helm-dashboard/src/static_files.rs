use std::path::PathBuf;

use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

pub fn service(static_dir: PathBuf) -> Router {
    let index = static_dir.join("index.html");
    Router::new().fallback_service(
        ServeDir::new(static_dir).not_found_service(ServeFile::new(index)),
    )
}
