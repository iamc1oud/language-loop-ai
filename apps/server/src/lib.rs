pub mod db;
pub mod ollama;
pub mod prompts;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State, rejection::JsonRejection},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use db::{Db, Message, Settings, Word};
use ollama::{Ollama, Res, Turn};
use serde::Deserialize;
use serde_json::{Value, json};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    services::ServeDir,
    set_header::SetResponseHeaderLayer,
};

#[derive(Clone)]
struct App {
    ollama: Ollama,
    db: Db,
}

pub fn router(ollama: String, model: String, db: Db, web: &str) -> Router {
    Router::new()
        .route("/api/convos", get(list_convos).post(create_convo))
        .route("/api/chat", post(chat))
        .route("/api/vocab", get(vocab).post(save_word))
        .route("/api/translate", post(translate))
        .route("/api/transcribe", post(transcribe))
        .route("/api/speak", post(speak))
        .route("/api/models", get(models))
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

fn internal(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

// ---------- Conversations ----------

async fn list_convos(State(app): State<App>) -> Res<impl IntoResponse> {
    Ok(Json(app.db.convos().map_err(internal)?))
}

#[derive(Deserialize)]
struct NewConvo {
    #[serde(flatten)]
    settings: Settings,
    /// Only set when importing conversations from the old localStorage format.
    updated: Option<i64>,
    #[serde(default)]
    messages: Vec<Message>,
}

async fn create_convo(State(app): State<App>, Json(r): Json<NewConvo>) -> Res<impl IntoResponse> {
    let t = r.updated.unwrap_or_else(now);
    let id = app.db.create_convo(&r.settings, t).map_err(internal)?.id;
    for m in &r.messages {
        app.db.add_message(id, m, t).map_err(internal)?;
    }
    Ok(Json(app.db.convo(id).map_err(internal)?))
}

#[derive(Deserialize)]
struct ChatReq {
    convo: i64,
    text: String,
    #[serde(default)]
    voice: bool,
    model: Option<String>,
}

/// Two model calls: reply in target language, then translate it to base.
/// The model has no structured output, so one call returning both is unreliable.
/// The user's message is saved first, so it survives an Ollama failure.
async fn chat(State(app): State<App>, req: Result<Json<ChatReq>, JsonRejection>) -> Res<impl IntoResponse> {
    // Usually a tab still running an older app.js (it sent the whole history, no convo id).
    let Json(req) = req.map_err(|e| {
        (e.status(), format!("This page is out of date: reload the page (Cmd+Shift+R). ({e})"))
    })?;
    let c = app
        .db
        .convo(req.convo)
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "no such conversation".into()))?;
    let Settings { base, target, level, .. } = &c.settings;

    // ponytail: full history every turn; trim/summarize when chats approach the context limit.
    // History keeps the student's raw words; instructions go only in the latest turn.
    let prompt = prompts::chat(&req.text, target, level);
    let mut turns: Vec<Turn> = c
        .messages
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .map(|m| Turn { role: &m.role, content: &m.content })
        .collect();
    turns.push(Turn { role: "user", content: &prompt });

    let user = Message { role: "user".into(), content: req.text.clone(), translation: None, voice: req.voice };
    app.db.add_message(c.id, &user, now()).map_err(internal)?;

    let model = req.model.as_deref().filter(|m| !m.is_empty());
    let reply = app.ollama.complete(model, &turns).await?;
    let tr_prompt = prompts::translate(&reply, target, base);
    let tr = app.ollama.complete(model, &[Turn { role: "user", content: &tr_prompt }]).await?;

    let m = Message { role: "assistant".into(), content: reply, translation: Some(tr), voice: false };
    app.db.add_message(c.id, &m, now()).map_err(internal)?;
    Ok(Json(json!({ "target": m.content, "base": m.translation })))
}

// ---------- Vocab ----------

#[derive(Deserialize)]
struct Lang {
    lang: String,
}

async fn vocab(State(app): State<App>, Query(q): Query<Lang>) -> Res<impl IntoResponse> {
    Ok(Json(app.db.vocab(&q.lang).map_err(internal)?))
}

#[derive(Deserialize)]
struct SaveWord {
    lang: String,
    #[serde(flatten)]
    word: Word,
}

async fn save_word(State(app): State<App>, Json(r): Json<SaveWord>) -> Res<StatusCode> {
    app.db.save_word(&r.lang, &r.word).map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- Model passthroughs ----------

#[derive(Deserialize)]
struct TranslateReq {
    text: String,
    from: String,
    to: String,
    model: Option<String>,
}

/// Word lookups from the UI. Returns {"text": "..."}.
async fn translate(State(app): State<App>, Json(r): Json<TranslateReq>) -> Res<Json<Value>> {
    let p = prompts::translate(&r.text, &r.from, &r.to);
    let model = r.model.as_deref().filter(|m| !m.is_empty());
    let text = app.ollama.complete(model, &[Turn { role: "user", content: &p }]).await?;
    Ok(Json(json!({ "text": text })))
}

/// Installed Ollama models, for the chat-model picker. Returns {"default": "...", "models": [...]}.
async fn models(State(app): State<App>) -> Res<Json<Value>> {
    Ok(Json(json!({ "default": app.ollama.model, "models": app.ollama.models().await? })))
}

/// Body: 16kHz mono WAV. Returns {"text": "..."}.
async fn transcribe(State(app): State<App>, wav: Bytes) -> Res<Json<Value>> {
    Ok(Json(app.ollama.transcribe(wav).await?))
}

#[derive(Deserialize)]
struct SpeakReq {
    text: String,
    voice: String,
}

/// Returns 24kHz WAV.
async fn speak(State(app): State<App>, Json(r): Json<SpeakReq>) -> Res<impl IntoResponse> {
    let wav = app.ollama.speak(&r.text, &r.voice).await?;
    Ok(([(header::CONTENT_TYPE, "audio/wav")], wav))
}
