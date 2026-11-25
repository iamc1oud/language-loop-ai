use super::{App, internal, now};
use crate::{
    db::{Message, Settings},
    ollama::Res,
};
use axum::{Json, extract::State, response::IntoResponse};
use serde::Deserialize;

pub async fn list(State(app): State<App>) -> Res<impl IntoResponse> {
    Ok(Json(app.db.convos().map_err(internal)?))
}

#[derive(Deserialize)]
pub struct NewConvo {
    #[serde(flatten)]
    settings: Settings,
    /// Only set when importing conversations from the old localStorage format.
    updated: Option<i64>,
    #[serde(default)]
    messages: Vec<Message>,
}

pub async fn create(State(app): State<App>, Json(r): Json<NewConvo>) -> Res<impl IntoResponse> {
    let t = r.updated.unwrap_or_else(now);
    let id = app.db.create_convo(&r.settings, t).map_err(internal)?.id;
    for m in &r.messages {
        app.db.add_message(id, m, t).map_err(internal)?;
    }
    Ok(Json(app.db.convo(id).map_err(internal)?))
}
