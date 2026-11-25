//! HTTP handlers, one module per resource.

pub mod chat;
pub mod convos;
pub mod models;
pub mod vocab;

use crate::{db::Db, ollama::Ollama};
use axum::http::StatusCode;

#[derive(Clone)]
pub struct App {
    pub ollama: Ollama,
    pub db: Db,
}

fn internal(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
