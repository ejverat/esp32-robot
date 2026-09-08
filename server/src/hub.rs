//! Hub central: mantiene el registro de robots y clientes conectados y enruta
//! los mensajes entre ellos.

use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

use crate::message::Message;

/// Mensaje saliente hacia un socket (texto JSON o binario para video).
#[derive(Debug, Clone)]
pub enum Outbound {
    Text(String),
    Binary(Vec<u8>),
}

impl Outbound {
    pub fn json(m: &Message) -> Self {
        Outbound::Text(serde_json::to_string(m).unwrap_or_default())
    }
}

pub enum HubCommand {
    RegisterRobot {
        robot_id: String,
        tx: mpsc::UnboundedSender<Outbound>,
    },
    UnregisterRobot {
        robot_id: String,
    },
    RegisterClient {
        client_id: u64,
        tx: mpsc::UnboundedSender<Outbound>,
    },
    UnregisterClient {
        client_id: u64,
    },
    /// Mensaje desde un cliente: enrutar a un robot concreto (`Some`) o a todos (`None`).
    RouteToRobot {
        robot_id: Option<String>,
        out: Outbound,
    },
    /// Mensaje desde un robot: difundir a todos los clientes.
    FromRobot {
        out: Outbound,
    },
    ListRobots {
        reply: oneshot::Sender<Vec<String>>,
    },
}

pub struct Hub {
    cmd_tx: mpsc::UnboundedSender<HubCommand>,
}

impl Hub {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        tokio::spawn(Self::run(cmd_rx));
        Self { cmd_tx }
    }

    pub fn command_tx(&self) -> mpsc::UnboundedSender<HubCommand> {
        self.cmd_tx.clone()
    }

    pub async fn list_robots(&self) -> Vec<String> {
        let (reply, rx) = oneshot::channel();
        let _ = self.cmd_tx.send(HubCommand::ListRobots { reply });
        rx.await.unwrap_or_default()
    }

    async fn run(mut rx: mpsc::UnboundedReceiver<HubCommand>) {
        let mut robots: HashMap<String, mpsc::UnboundedSender<Outbound>> = HashMap::new();
        let mut clients: HashMap<u64, mpsc::UnboundedSender<Outbound>> = HashMap::new();

        while let Some(cmd) = rx.recv().await {
            match cmd {
                HubCommand::RegisterRobot { robot_id, tx } => {
                    if robots.insert(robot_id.clone(), tx).is_some() {
                        eprintln!("[hub] robot '{robot_id}' reconectado (reemplazado)");
                    }
                }
                HubCommand::UnregisterRobot { robot_id } => {
                    robots.remove(&robot_id);
                }
                HubCommand::RegisterClient { client_id, tx } => {
                    clients.insert(client_id, tx);
                }
                HubCommand::UnregisterClient { client_id } => {
                    clients.remove(&client_id);
                }
                HubCommand::RouteToRobot { robot_id, out } => match robot_id {
                    Some(id) => {
                        if let Some(tx) = robots.get(&id) {
                            if tx.send(out).is_err() {
                                eprintln!("[hub] no se pudo enviar a robot '{id}'");
                            }
                        } else {
                            eprintln!("[hub] robot '{id}' no conectado; mensaje descartado");
                        }
                    }
                    None => {
                        // Sin robot_id: difundir a todos (p. ej. "detener flota").
                        for tx in robots.values() {
                            let _ = tx.send(out.clone());
                        }
                    }
                },
                HubCommand::FromRobot { out } => {
                    for tx in clients.values() {
                        let _ = tx.send(out.clone());
                    }
                }
                HubCommand::ListRobots { reply } => {
                    let mut list: Vec<String> = robots.keys().cloned().collect();
                    list.sort();
                    let _ = reply.send(list);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::MessageType;

    #[tokio::test]
    async fn routes_client_to_robot_and_robot_to_clients() {
        let hub = Hub::new();
        let (robot_tx, mut robot_rx) = mpsc::unbounded_channel();
        let (client_tx, mut client_rx) = mpsc::unbounded_channel();

        hub.command_tx()
            .send(HubCommand::RegisterRobot { robot_id: "a1".into(), tx: robot_tx })
            .unwrap();
        hub.command_tx()
            .send(HubCommand::RegisterClient { client_id: 1, tx: client_tx })
            .unwrap();

        // cliente -> robot a1
        let cmd = Message {
            kind: MessageType::CmdMove,
            id: None,
            ts: None,
            robot_id: Some("a1".into()),
            payload: serde_json::json!({ "v": 0.5, "omega": 0.2 }),
        };
        hub.command_tx()
            .send(HubCommand::RouteToRobot {
                robot_id: Some("a1".into()),
                out: Outbound::json(&cmd),
            })
            .unwrap();

        let got = robot_rx.recv().await.expect("robot debería recibir el comando");
        let Outbound::Text(t) = got else { panic!("esperaba texto") };
        assert!(t.contains("cmd:move") && t.contains("a1"));

        // robot -> clientes (telemetría)
        hub.command_tx()
            .send(HubCommand::FromRobot {
                out: Outbound::Text(
                    r#"{"type":"telemetry","robot_id":"a1","payload":{"battery":3.9}}"#.into(),
                ),
            })
            .unwrap();
        let got = client_rx.recv().await.expect("cliente debería recibir telemetría");
        assert!(matches!(got, Outbound::Text(_)));

        // lista de robots
        assert_eq!(hub.list_robots().await, vec!["a1".to_string()]);
    }

    #[tokio::test]
    async fn broadcast_when_no_robot_id() {
        let hub = Hub::new();
        let (tx1, mut rx1) = mpsc::unbounded_channel();
        let (tx2, mut rx2) = mpsc::unbounded_channel();
        hub.command_tx()
            .send(HubCommand::RegisterRobot { robot_id: "a1".into(), tx: tx1 })
            .unwrap();
        hub.command_tx()
            .send(HubCommand::RegisterRobot { robot_id: "b2".into(), tx: tx2 })
            .unwrap();

        let stop = Message {
            kind: MessageType::CmdStop,
            id: None,
            ts: None,
            robot_id: None,
            payload: serde_json::json!({}),
        };
        hub.command_tx()
            .send(HubCommand::RouteToRobot {
                robot_id: None,
                out: Outbound::json(&stop),
            })
            .unwrap();

        assert!(rx1.recv().await.is_some());
        assert!(rx2.recv().await.is_some());
    }
}
