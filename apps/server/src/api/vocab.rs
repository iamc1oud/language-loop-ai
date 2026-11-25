use super::{App, internal};
use crate::{db::Word, ollama::Res};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Lang {
    lang: String,
}

pub async fn list(State(app): State<App>, Query(q): Query<Lang>) -> Res<impl IntoResponse> {
    Ok(Json(app.db.vocab(&q.lang).map_err(internal)?))
}

#[derive(Deserialize)]
pub struct SaveWord {
    lang: String,
    #[serde(flatten)]
    word: Word,
}

pub async fn save(State(app): State<App>, Json(r): Json<SaveWord>) -> Res<StatusCode> {
    app.db.save_word(&r.lang, &r.word).map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}
