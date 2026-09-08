use axum::{routing::get, Router};

#[tokio::main]
async fn main() {
    let app = Router::new().route("/health", get(|| async { "ok\n" }));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .expect("no se pudo enlazar el puerto 8080");

    println!("🦀 servidor escuchando en http://127.0.0.1:8080 (health en /health)");
    axum::serve(listener, app).await.expect("error del servidor");
}
