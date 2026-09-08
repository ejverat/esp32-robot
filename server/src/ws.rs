//! Handlers WebSocket para robots y clientes.

use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

use crate::hub::{HubCommand, Outbound};
use crate::message::{Message, RouteTarget};
use crate::AppState;

#[derive(Deserialize)]
pub struct RobotParams {
    pub robot_id: Option<String>,
}

/// Conexión de un robot: `GET /ws/robot?robot_id=<id>`.
pub async fn robot_ws(
    ws: WebSocketUpgrade,
    Query(params): Query<RobotParams>,
    State(state): State<AppState>,
) -> Response {
    let robot_id = params.robot_id.unwrap_or_default().trim().to_string();
    if robot_id.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, "falta ?robot_id=<id>\n").into_response();
    }
    ws.on_upgrade(move |socket| robot_session(socket, state, robot_id))
}

/// Conexión de un cliente (navegador): `GET /ws/client`.
pub async fn client_ws(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| client_session(socket, state))
}

async fn robot_session(socket: WebSocket, state: AppState, robot_id: String) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Outbound>();
    let cmd_tx = state.hub.command_tx();

    let _ = cmd_tx.send(HubCommand::RegisterRobot {
        robot_id: robot_id.clone(),
        tx: out_tx,
    });

    // outbound -> socket
    let forward = tokio::spawn(async move {
        while let Some(out) = out_rx.recv().await {
            let msg = match out {
                Outbound::Text(t) => WsMessage::Text(t.into()),
                Outbound::Binary(b) => WsMessage::Binary(b.into()),
            };
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // inbound -> hub (difundir a clientes)
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            WsMessage::Text(t) => {
                // Se reenvía tal cual; el robot debe incluir `robot_id`.
                let _ = cmd_tx.send(HubCommand::FromRobot {
                    out: Outbound::Text(t.to_string()),
                });
            }
            WsMessage::Binary(b) => {
                // Frame de video: passthrough binario a los clientes.
                let _ = cmd_tx.send(HubCommand::FromRobot {
                    out: Outbound::Binary(b.to_vec()),
                });
            }
            WsMessage::Close(_) => break,
            _ => {}
        }
    }

    let _ = cmd_tx.send(HubCommand::UnregisterRobot {
        robot_id: robot_id.clone(),
    });
    forward.abort();
}

async fn client_session(socket: WebSocket, state: AppState) {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let client_id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Outbound>();
    let local_tx = out_tx.clone();
    let cmd_tx = state.hub.command_tx();

    let _ = cmd_tx.send(HubCommand::RegisterClient { client_id, tx: out_tx });

    // Al conectar, enviar la lista de robots activos.
    let robots = state.hub.list_robots().await;
    let _ = local_tx.send(Outbound::json(&Message::robots_list(robots)));

    // outbound -> socket
    let forward = tokio::spawn(async move {
        while let Some(out) = out_rx.recv().await {
            let msg = match out {
                Outbound::Text(t) => WsMessage::Text(t.into()),
                Outbound::Binary(b) => WsMessage::Binary(b.into()),
            };
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // inbound -> hub (enrutar a robot por robot_id)
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            WsMessage::Text(t) => {
                let target: RouteTarget = match serde_json::from_str(t.as_str()) {
                    Ok(x) => x,
                    Err(e) => {
                        let _ = local_tx
                            .send(Outbound::json(&Message::error(format!("JSON inválido: {e}"))));
                        continue;
                    }
                };
                let _ = cmd_tx.send(HubCommand::RouteToRobot {
                    robot_id: target.robot_id,
                    out: Outbound::Text(t.to_string()),
                });
            }
            WsMessage::Close(_) => break,
            _ => {}
        }
    }

    let _ = cmd_tx.send(HubCommand::UnregisterClient { client_id });
    forward.abort();
}
