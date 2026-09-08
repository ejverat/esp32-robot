mod hub;
mod message;
mod ws;

use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};

use crate::hub::Hub;

#[derive(Clone)]
struct AppState {
    hub: Arc<Hub>,
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/robots", get(list_robots))
        .route("/ws/robot", get(ws::robot_ws))
        .route("/ws/client", get(ws::client_ws))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let state = AppState {
        hub: Arc::new(Hub::new()),
    };

    let app = build_router(state);

    let addr = std::env::var("HUB_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("no se pudo enlazar {addr}: {e}"));

    println!("🦀 hub escuchando en http://{addr}");
    println!("   robots:  ws://<host>:8080/ws/robot?robot_id=<id>");
    println!("   clientes: ws://<host>:8080/ws/client");
    println!("   estado:  GET /robots");

    axum::serve(listener, app).await.expect("error del servidor");
}

async fn health() -> &'static str {
    "ok\n"
}

async fn list_robots(State(state): State<AppState>) -> Json<serde_json::Value> {
    let robots = state.hub.list_robots().await;
    Json(serde_json::json!({ "robots": robots }))
}

#[cfg(test)]
mod e2e {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as TwsMsg;

    #[tokio::test]
    async fn robot_and_client_route_messages() {
        let state = AppState {
            hub: Arc::new(Hub::new()),
        };
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let (mut robot, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws/robot?robot_id=a1"))
            .await
            .unwrap();
        let (mut client, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws/client"))
            .await
            .unwrap();

        // lista de robots al conectar
        let m = client.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(m.contains("robots:list"));

        // cliente -> robot
        client
            .send(TwsMsg::Text(
                r#"{"type":"cmd:move","robot_id":"a1","payload":{"v":0.5}}"#.into(),
            ))
            .await
            .unwrap();
        let r = robot.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(r.contains("cmd:move"));

        // robot -> telemetría -> cliente
        robot
            .send(TwsMsg::Text(
                r#"{"type":"telemetry","robot_id":"a1","payload":{"battery":3.9}}"#.into(),
            ))
            .await
            .unwrap();
        let c = client.next().await.unwrap().unwrap().into_text().unwrap();
        assert!(c.contains("telemetry"));
    }
}
