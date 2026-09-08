//! Esquema de mensajes del hub (envelope JSON versionado).
//!
//! Formato del envelope:
//! ```json
//! {
//!   "type": "cmd:move",
//!   "id": "c1f2-...",
//!   "ts": 1725123456,
//!   "robot_id": "a1",
//!   "payload": { "v": 0.5, "omega": 0.2 }
//! }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Tipo de mensaje (campo `type` del envelope).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    #[serde(rename = "cmd:move")]
    CmdMove,
    #[serde(rename = "cmd:stop")]
    CmdStop,
    #[serde(rename = "cmd:camera")]
    CmdCamera,
    #[serde(rename = "telemetry")]
    Telemetry,
    #[serde(rename = "video")]
    Video,
    #[serde(rename = "config")]
    Config,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "register")]
    Register,
    #[serde(rename = "robots:list")]
    RobotsList,
    #[serde(rename = "error")]
    Error,
}

/// Envelope de mensaje. El hub actúa como *relay*: no interpreta `payload`,
/// solo enruta por `robot_id` y por la dirección de la conexión.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    #[serde(rename = "type")]
    pub kind: MessageType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub robot_id: Option<String>,
    #[serde(default)]
    pub payload: Value,
}

impl Message {
    /// `robots:list`: lista de robots activos, enviada por el hub a los clientes.
    pub fn robots_list(robots: Vec<String>) -> Self {
        Message {
            kind: MessageType::RobotsList,
            id: None,
            ts: Some(now_millis()),
            robot_id: None,
            payload: serde_json::json!({ "robots": robots }),
        }
    }

    /// `error`: mensaje de error emitido por el hub.
    pub fn error(message: impl Into<String>) -> Self {
        Message {
            kind: MessageType::Error,
            id: None,
            ts: Some(now_millis()),
            robot_id: None,
            payload: serde_json::json!({ "message": message.into() }),
        }
    }
}

/// Estructura mínima para extraer el `robot_id` de un mensaje entrante de un
/// cliente (sin depender del resto del esquema, para tolerar tipos futuros).
#[derive(Debug, Deserialize)]
pub struct RouteTarget {
    #[serde(default)]
    pub robot_id: Option<String>,
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
