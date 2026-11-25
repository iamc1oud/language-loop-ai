mod api;
pub mod db;
pub mod ollama;
pub mod prompts;

use api::App;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::header,
    routing::{get, post},
};
use db::Db;
use ollama::Ollama;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    services::ServeDir,
    set_header::SetResponseHeaderLayer,
};

pub fn router(ollama: String, model: String, db: Db, web: &str) -> Router {
    Router::new()
        .route("/api/convos", get(api::convos::list).post(api::convos::create))
        .route("/api/chat", post(api::chat::chat))
        .route("/api/vocab", get(api::vocab::list).post(api::vocab::save))
        .route("/api/translate", post(api::models::translate))
        .route("/api/transcribe", post(api::models::transcribe))
        .route("/api/speak", post(api::models::speak))
        .route("/api/models", get(api::models::models))
        .layer(DefaultBodyLimit::max(20 * 1024 * 1024)) // ~10 min of 16kHz WAV
        .layer(cors())
        // no-cache = revalidate every load (cheap 304 via ETag), so a stale bundle never
        // talks to a newer API.
        .fallback_service(
            tower::ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::overriding(
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static("no-cache"),
                ))
                .service(ServeDir::new(web)),
        )
        .with_state(App { ollama: Ollama::new(ollama, model), db })
}

/// For when the UI is opened from file:// or a dev server on another port.
/// Only local origins: any website could otherwise drive your Ollama through this API.
fn cors() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|o, _| {
            let o = o.as_bytes();
            o == b"null"
                || ["http://localhost", "http://127.0.0.1"].iter().any(|h| {
                    o.strip_prefix(h.as_bytes())
                        .is_some_and(|rest| rest.is_empty() || rest[0] == b':')
                })
        }))
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers([header::CONTENT_TYPE])
}
