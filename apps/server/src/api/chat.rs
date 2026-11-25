use super::{App, internal, now};
use crate::{
    db::{Message, Settings},
    ollama::{Res, Turn},
    prompts,
};
use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub struct ChatReq {
    convo: i64,
    text: String,
    #[serde(default)]
    voice: bool,
    model: Option<String>,
}

/// Two model calls: reply in target language, then translate it to base.
/// The model has no structured output, so one call returning both is unreliable.
/// The user's message is saved first, so it survives an Ollama failure.
pub async fn chat(State(app): State<App>, req: Result<Json<ChatReq>, JsonRejection>) -> Res<impl IntoResponse> {
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
