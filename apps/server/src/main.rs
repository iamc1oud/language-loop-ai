fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.into())
}

#[tokio::main]
async fn main() {
    let db_path = env(
        "DB_PATH",
        concat!(env!("CARGO_MANIFEST_DIR"), "/language-ai.db"),
    );
    let db = server::db::Db::open(&db_path).expect("open database");
    let router = server::router(
        env("OLLAMA_URL", "http://localhost:11434"),
        env("MODEL", "lfm3-audio:latest"),
        db,
        &env("WEB_DIR", concat!(env!("CARGO_MANIFEST_DIR"), "/../web/dist")),
    );
    let addr = env("ADDR", "127.0.0.1:3000");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("listening on http://{addr} (db: {db_path})");
    axum::serve(listener, router).await.unwrap();
}
