//! HTTP-level tests: the real router on an ephemeral port, talking to a fake Ollama.

use axum::{Json, Router, http::StatusCode, routing::post};
use serde_json::{Value, json};
use server::db::Db;
use std::sync::{Arc, Mutex};

type Seen = Arc<Mutex<Vec<Value>>>;

async fn serve(router: Router) -> String {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, router).await.unwrap() });
    url
}

/// Fake Ollama /api/chat: records each request; translation prompts get "TR:<text>", chat gets "REPLY<n>".
async fn fake_ollama() -> (String, Seen) {
    let seen: Seen = Default::default();
    let s = seen.clone();
    let router = Router::new()
        .route(
            "/api/chat",
            post(move |Json(body): Json<Value>| {
                let s = s.clone();
                async move {
                    let last = body["messages"].as_array().unwrap().last().unwrap()["content"]
                        .as_str()
                        .unwrap()
                        .to_string();
                    let mut s = s.lock().unwrap();
                    s.push(body);
                    let content = if last.starts_with("You are a professional") {
                        format!("TR:{}", last.rsplit('\n').next().unwrap())
                    } else {
                        format!("REPLY{}", s.len())
                    };
                    Json(json!({ "message": { "role": "assistant", "content": content } }))
                }
            }),
        )
        .route(
            "/api/tags",
            axum::routing::get(|| async { Json(json!({ "models": [{ "name": "a:1" }, { "name": "b:2" }] })) }),
        );
    (serve(router).await, seen)
}

async fn app(ollama: &str) -> String {
    let db = Db::open(":memory:").unwrap();
    serve(server::router(ollama.into(), "default:latest".into(), db, ".")).await
}

async fn post_json(url: String, body: Value) -> (StatusCode, Value) {
    let r = reqwest::Client::new().post(url).json(&body).send().await.unwrap();
    let status = r.status();
    (status, r.json().await.unwrap_or(Value::Null))
}

async fn get_json(url: String) -> Value {
    reqwest::get(url).await.unwrap().json().await.unwrap()
}

async fn new_convo(base: &str) -> i64 {
    let (s, c) = post_json(
        format!("{base}/api/convos"),
        json!({ "base": "English", "target": "Spanish", "level": "Beginner", "mode": "Text only" }),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(c["messages"], json!([]));
    c["id"].as_i64().unwrap()
}

#[tokio::test]
async fn chat_returns_reply_with_translation_and_persists_both_turns() {
    let (ollama, _) = fake_ollama().await;
    let base = app(&ollama).await;
    let id = new_convo(&base).await;

    let (s, r) = post_json(format!("{base}/api/chat"), json!({ "convo": id, "text": "hola", "voice": true })).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(r, json!({ "target": "REPLY1", "base": "TR:REPLY1" }));

    let convos = get_json(format!("{base}/api/convos")).await;
    assert_eq!(convos[0]["id"], id);
    assert_eq!(
        convos[0]["messages"],
        json!([
            { "role": "user", "content": "hola", "voice": true },
            { "role": "assistant", "content": "REPLY1", "translation": "TR:REPLY1", "voice": false }
        ])
    );
}

#[tokio::test]
async fn chat_sends_stored_history_and_uses_convo_settings() {
    let (ollama, seen) = fake_ollama().await;
    let base = app(&ollama).await;
    let id = new_convo(&base).await;
    post_json(format!("{base}/api/chat"), json!({ "convo": id, "text": "hola" })).await;
    post_json(format!("{base}/api/chat"), json!({ "convo": id, "text": "adiós", "model": "b:2" })).await;

    let seen = seen.lock().unwrap();
    let second_chat = &seen[2]; // [chat1, translate1, chat2, translate2]
    assert_eq!(second_chat["model"], "b:2");
    let msgs = second_chat["messages"].as_array().unwrap();
    assert_eq!(msgs[0], json!({ "role": "user", "content": "hola" }));
    assert_eq!(msgs[1], json!({ "role": "assistant", "content": "REPLY1" }));
    let prompt = msgs[2]["content"].as_str().unwrap();
    assert!(prompt.contains("The user says: \"adiós\""));
    assert!(prompt.contains("Always reply only in Spanish"));
    assert_eq!(msgs.len(), 3);
    assert_eq!(seen[0]["model"], "default:latest", "no model picked → server default");
}

#[tokio::test]
async fn chat_with_unknown_convo_is_404() {
    let (ollama, _) = fake_ollama().await;
    let base = app(&ollama).await;
    let (s, _) = post_json(format!("{base}/api/chat"), json!({ "convo": 999, "text": "hola" })).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn ollama_failure_is_502_and_keeps_the_user_message() {
    let base = app("http://127.0.0.1:9").await; // nothing listens on the discard port
    let id = new_convo(&base).await;
    let (s, _) = post_json(format!("{base}/api/chat"), json!({ "convo": id, "text": "hola" })).await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
    let convos = get_json(format!("{base}/api/convos")).await;
    assert_eq!(convos[0]["messages"], json!([{ "role": "user", "content": "hola", "voice": false }]));
}

#[tokio::test]
async fn creating_a_convo_with_messages_imports_them() {
    let (ollama, _) = fake_ollama().await;
    let base = app(&ollama).await;
    let msgs = json!([
        { "role": "user", "content": "hola", "voice": false },
        { "role": "assistant", "content": "¡Hola!", "translation": "Hi!", "voice": false }
    ]);
    let (s, c) = post_json(
        format!("{base}/api/convos"),
        json!({ "base": "English", "target": "Spanish", "level": "Beginner", "mode": "Text only",
                "updated": 1234, "messages": msgs }),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(c["messages"], msgs);
    assert_eq!(c["updated"], 1234);
}

#[tokio::test]
async fn vocab_round_trips_per_language() {
    let (ollama, _) = fake_ollama().await;
    let base = app(&ollama).await;
    let save = |lang: &str, word: &str, gloss: &str| {
        post_json(format!("{base}/api/vocab"), json!({ "lang": lang, "word": word, "gloss": gloss }))
    };
    assert_eq!(save("Spanish", "perro", "dog").await.0, StatusCode::NO_CONTENT);
    save("Spanish", "gato", "cat").await;
    save("French", "chat", "cat").await;
    assert_eq!(
        get_json(format!("{base}/api/vocab?lang=Spanish")).await,
        json!([{ "word": "gato", "gloss": "cat" }, { "word": "perro", "gloss": "dog" }])
    );
}

#[tokio::test]
async fn models_lists_installed_names_and_default() {
    let (ollama, _) = fake_ollama().await;
    let base = app(&ollama).await;
    assert_eq!(
        get_json(format!("{base}/api/models")).await,
        json!({ "default": "default:latest", "models": ["a:1", "b:2"] })
    );
}

#[tokio::test]
async fn cors_allows_only_local_origins() {
    let (ollama, _) = fake_ollama().await;
    let base = app(&ollama).await;
    let acao = |origin: &'static str| {
        let url = format!("{base}/api/models");
        async move {
            reqwest::Client::new()
                .get(url)
                .header("origin", origin)
                .send()
                .await
                .unwrap()
                .headers()
                .get("access-control-allow-origin")
                .map(|v| v.to_str().unwrap().to_string())
        }
    };
    assert_eq!(acao("http://localhost:5173").await.as_deref(), Some("http://localhost:5173"));
    assert_eq!(acao("null").await.as_deref(), Some("null"));
    assert_eq!(acao("http://localhost.evil.com").await, None);
    assert_eq!(acao("https://example.com").await, None);
}

/// Regression: without Cache-Control the browser may reuse a stale app.js after a server
/// upgrade and send the old /api/chat body ("missing field `convo`").
#[tokio::test]
async fn static_files_are_revalidated_on_every_load() {
    let (ollama, _) = fake_ollama().await;
    let base = app(&ollama).await; // web dir "." = crate dir under `cargo test`
    let r = reqwest::get(format!("{base}/Cargo.toml")).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(r.headers().get("cache-control").unwrap(), "no-cache");
}

/// A tab still running an older app.js sends the old body (history, no convo id). Tell the
/// user to reload instead of a serde error they can't act on.
#[tokio::test]
async fn chat_from_outdated_page_asks_for_reload() {
    let (ollama, _) = fake_ollama().await;
    let base = app(&ollama).await;
    let r = reqwest::Client::new()
        .post(format!("{base}/api/chat"))
        .json(&json!({ "base": "English", "target": "Spanish", "level": "beginner", "history": [], "text": "hola" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let msg = r.text().await.unwrap();
    assert!(msg.contains("reload the page"), "{msg}");
}
