//! Punto de entrada: resuelve la configuración, conecta WiFi como estación y
//! mantiene vivo el cliente WebSocket (auto-reconexión por la librería C).

mod command;
mod config;
mod motors;
mod net;
mod wifi;
mod ws_client;

use std::sync::mpsc;
use std::thread;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};
use esp_idf_svc::sys::EspError;

use crate::config::DEFAULT_HUB_URL;
use crate::motors::MotorSignal;
use crate::net::NetEvent;

fn main() -> Result<(), EspError> {
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs_partition = EspDefaultNvsPartition::take()?;

    let nvs = EspNvs::new(nvs_partition.clone(), config::NVS_NAMESPACE, true)?;
    let config = config::resolve(&nvs)?;
    drop(nvs);

    let (tx, rx) = mpsc::channel::<NetEvent>();
    let (motor_tx, motor_rx) = mpsc::channel::<MotorSignal>();

    let motors = motors::init(
        peripherals.ledc,
        peripherals.pins.gpio12,
        peripherals.pins.gpio13,
        peripherals.pins.gpio14,
        peripherals.pins.gpio15,
    )?;
    thread::spawn(move || motors::run(motor_rx, motors));

    let mut wifi = wifi::init(peripherals.modem, &sysloop, nvs_partition, &config, tx.clone())?;
    wifi::connect(&mut wifi)?;

    let uri = match ws_client::build_uri(&config) {
        Ok(uri) => uri,
        Err(e) => {
            log::error!("invalid hub.url ({e:?}); falling back to {DEFAULT_HUB_URL}");
            DEFAULT_HUB_URL.to_string()
        }
    };

    let _ws = ws_client::start(&uri, &ws_client::client_config(), tx.clone(), motor_tx.clone())
        .map_err(|e| e.0)?;

    loop {
        match rx.recv() {
            Ok(NetEvent::WifiStaDisconnected) => wifi::connect(&mut wifi)?,
            Ok(NetEvent::WsEvent) => {}
            Err(_) => {
                log::warn!("event senders dropped; parking");
                thread::park();
            }
        }
    }
}
