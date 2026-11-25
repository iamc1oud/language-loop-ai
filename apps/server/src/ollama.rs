//! Thin client for the Ollama endpoints we use. Every error maps to 502.

use axum::{body::Bytes, http::StatusCode};
use serde::Serialize;
use serde_json::{Value, json};

pub type Res<T> = Result<T, (StatusCode, String)>;

pub fn bad(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::BAD_GATEWAY, e.to_string())
}

async fn ok(r: reqwest::Response) -> Res<reqwest::Response> {
    if r.status().is_success() {
        Ok(r)
    } else {
        Err(bad(format!("ollama {}: {}", r.status(), r.text().await.unwrap_or_default())))
    }
}

#[derive(Serialize)]
pub struct Turn<'a> {
    pub role: &'a str,
    pub content: &'a str,
}

#[derive(Clone)]
pub struct Ollama {
    http: reqwest::Client,
    url: String,
    /// Default model: speech and transcription always use it; chat uses it when none is picked.
    pub model: String,
}

impl Ollama {
    pub fn new(url: String, model: String) -> Self {
        Self { http: reqwest::Client::new(), url, model }
    }

    pub async fn complete(&self, model: Option<&str>, messages: &[Turn<'_>]) -> Res<String> {
        let r = self
            .http
            .post(format!("{}/api/chat", self.url))
            .json(&json!({ "model": model.unwrap_or(&self.model), "stream": false, "messages": messages }))
            .send()
            .await
            .map_err(bad)?;
        let v: Value = ok(r).await?.json().await.map_err(bad)?;
        v["message"]["content"].as_str().map(|s| s.trim().to_string()).ok_or_else(|| bad(v))
    }

    pub async fn models(&self) -> Res<Vec<String>> {
        let r = self.http.get(format!("{}/api/tags", self.url)).send().await.map_err(bad)?;
        let v: Value = ok(r).await?.json().await.map_err(bad)?;
        Ok(v["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["name"].as_str().map(String::from))
            .collect())
    }

    /// 16kHz mono WAV in, Ollama's {"text": "..."} out.
    pub async fn transcribe(&self, wav: Bytes) -> Res<Value> {
        let file = reqwest::multipart::Part::bytes(wav.to_vec())
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(bad)?;
        let form = reqwest::multipart::Form::new().text("model", self.model.clone()).part("file", file);
        let r = self
            .http
            .post(format!("{}/v1/audio/transcriptions", self.url))
            .multipart(form)
            .send()
            .await
            .map_err(bad)?;
        ok(r).await?.json().await.map_err(bad)
    }

    /// 24kHz WAV.
    pub async fn speak(&self, text: &str, voice: &str) -> Res<Bytes> {
        let r = self
            .http
            .post(format!("{}/v1/audio/speech", self.url))
            .json(&json!({ "model": self.model, "input": text, "voice": voice }))
            .send()
            .await
            .map_err(bad)?;
        ok(r).await?.bytes().await.map_err(bad)
    }
}
