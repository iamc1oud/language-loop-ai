//! Thin passthroughs to Ollama.

use super::App;
use crate::{
    ollama::{Res, Turn},
    prompts,
};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::header,
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
pub struct TranslateReq {
    text: String,
    from: String,
    to: String,
    model: Option<String>,
}

/// Word lookups from the UI. Returns {"text": "..."}.
pub async fn translate(State(app): State<App>, Json(r): Json<TranslateReq>) -> Res<Json<Value>> {
    let p = prompts::translate(&r.text, &r.from, &r.to);
    let model = r.model.as_deref().filter(|m| !m.is_empty());
    let text = app.ollama.complete(model, &[Turn { role: "user", content: &p }]).await?;
    Ok(Json(json!({ "text": text })))
}

/// Installed Ollama models, for the chat-model picker. Returns {"default": "...", "models": [...]}.
pub async fn models(State(app): State<App>) -> Res<Json<Value>> {
    Ok(Json(json!({ "default": app.ollama.model, "models": app.ollama.models().await? })))
}

/// Body: 16kHz mono WAV. Returns {"text": "..."}.
pub async fn transcribe(State(app): State<App>, wav: Bytes) -> Res<Json<Value>> {
    Ok(Json(app.ollama.transcribe(wav).await?))
}

#[derive(Deserialize)]
pub struct SpeakReq {
    text: String,
    voice: String,
}

/// Returns 24kHz WAV.
pub async fn speak(State(app): State<App>, Json(r): Json<SpeakReq>) -> Res<impl IntoResponse> {
    let wav = app.ollama.speak(&r.text, &r.voice).await?;
    Ok(([(header::CONTENT_TYPE, "audio/wav")], wav))
}
