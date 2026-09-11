//! Eventos de red compartidos entre `wifi`, `ws_client` y `main`.
//! Ningún módulo depende de otro: todos se comunican a través de este enum.

use std::sync::mpsc::Sender;

/// Evento que los módulos de red publican hacia el bucle principal.
pub enum NetEvent {
    WifiStaDisconnected,
    WsEvent,
}

/// Canal no acotado, igual que el ejemplo `http_ws_client` de upstream.
pub type EventSender = Sender<NetEvent>;
