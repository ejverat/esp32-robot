//! Cliente WebSocket: URI de marcado, configuración explícita y callback de eventos.

use std::time::Duration;

use esp_idf_svc::io::EspIOError;
use esp_idf_svc::ws::client::{
    EspWebSocketClient, EspWebSocketClientConfig, EspWebSocketTransport, WebSocketEvent,
    WebSocketEventType,
};

use crate::config::Config;
use crate::motors::{MotorSender, MotorSignal};
use crate::net::{EventSender, NetEvent};

#[derive(Debug)]
pub enum UriError {
    MissingPort,
    InvalidPort,
    EmptyHost,
}

pub fn build_uri(config: &Config) -> Result<String, UriError> {
    let (host, port_str) = config.hub_url.rsplit_once(':').ok_or(UriError::MissingPort)?;
    if host.is_empty() {
        return Err(UriError::EmptyHost);
    }
    let port = port_str.parse::<u16>().map_err(|_| UriError::InvalidPort)?;
    let robot_id = percent_encode(&config.robot_id);
    Ok(format!("ws://{host}:{port}/ws/robot?robot_id={robot_id}"))
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => {
                out.push('%');
                out.push(hex(byte >> 4));
                out.push(hex(byte & 0x0F));
            }
        }
    }
    out
}

fn hex(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'A' + (nibble - 10)) as char,
    }
}

pub fn client_config() -> EspWebSocketClientConfig<'static> {
    EspWebSocketClientConfig {
        transport: EspWebSocketTransport::TransportOverTCP,
        task_prio: 5,
        task_stack: 4096,
        buffer_size: 1024,
        ping_interval_sec: Duration::from_secs(10),
        pingpong_timeout_sec: Duration::from_secs(60),
        reconnect_timeout_ms: Duration::from_millis(3000),
        network_timeout_ms: Duration::from_millis(10000),
        disable_auto_reconnect: false,
        disable_pingpong_discon: false,
        ..Default::default()
    }
}

pub fn start(
    uri: &str,
    cfg: &EspWebSocketClientConfig<'static>,
    tx: EventSender,
    motor_tx: MotorSender,
) -> Result<EspWebSocketClient<'static>, EspIOError> {
    EspWebSocketClient::new(uri, cfg, Duration::from_secs(5), move |event| {
        log_event(event);
        tx.send(NetEvent::WsEvent).ok();
        if let Ok(event) = event {
            match &event.event_type {
                WebSocketEventType::Text(s) => {
                    motor_tx.send(MotorSignal::Text(s.to_string())).ok();
                }
                WebSocketEventType::Disconnected
                | WebSocketEventType::Close(_)
                | WebSocketEventType::Closed => {
                    motor_tx.send(MotorSignal::ConnectionLost).ok();
                }
                _ => {}
            }
        }
    })
}

pub fn log_event(event: &Result<WebSocketEvent, EspIOError>) {
    match event {
        Ok(event) => match &event.event_type {
            WebSocketEventType::BeforeConnect => log::debug!("WebSocket before connect"),
            WebSocketEventType::Connected => log::info!("WebSocket connected"),
            WebSocketEventType::Disconnected => {
                log::info!("WebSocket disconnected (auto-reconnect pending)")
            }
            WebSocketEventType::Close(reason) => log::info!("WebSocket close, reason: {reason:?}"),
            WebSocketEventType::Closed => log::info!("WebSocket closed"),
            WebSocketEventType::Text(_) => log::debug!("WebSocket text received"),
            WebSocketEventType::Binary(_) => log::debug!("WebSocket binary received"),
            WebSocketEventType::Ping => log::debug!("WebSocket ping"),
            WebSocketEventType::Pong => log::debug!("WebSocket pong"),
        },
        Err(e) => log::warn!("WebSocket error: {e}"),
    }
}
