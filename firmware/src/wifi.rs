//! WiFi en modo estación (STA) con reconexión por retroceso acotado.

use std::thread::sleep;
use std::time::Duration;

use esp_idf_svc::eventloop::{EspSubscription, EspSystemEventLoop, System};
use esp_idf_svc::hal::modem::WifiModemPeripheral;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys::{EspError, ESP_ERR_NO_MEM};
use esp_idf_svc::wifi::{
    AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi, WifiEvent,
};

use crate::config::Config;
use crate::net::{EventSender, NetEvent};

const AUTH_METHOD: AuthMethod = AuthMethod::WPA2Personal;
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const BACKOFF_CAP: Duration = Duration::from_secs(15);

pub struct Wifi {
    blocking: BlockingWifi<EspWifi<'static>>,
    _sub: EspSubscription<'static, System>,
}

pub fn init<M: WifiModemPeripheral + 'static>(
    modem: M,
    sysloop: &EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    config: &Config,
    tx: EventSender,
) -> Result<Wifi, EspError> {
    let esp_wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;
    let mut blocking = BlockingWifi::wrap(esp_wifi, sysloop.clone())?;

    let client_config = ClientConfiguration {
        ssid: config
            .ssid
            .as_str()
            .try_into()
            .map_err(|_| EspError::from_infallible::<ESP_ERR_NO_MEM>())?,
        password: config
            .password
            .as_str()
            .try_into()
            .map_err(|_| EspError::from_infallible::<ESP_ERR_NO_MEM>())?,
        auth_method: AUTH_METHOD,
        ..Default::default()
    };

    blocking.set_configuration(&Configuration::Client(client_config))?;
    blocking.start()?;

    let sub = sysloop.subscribe::<WifiEvent, _>(move |event| {
        if matches!(event, WifiEvent::StaDisconnected(_)) {
            log::info!("WiFi STA disconnected");
            tx.send(NetEvent::WifiStaDisconnected).ok();
        }
    })?;

    log::info!("WiFi started (STA)");

    Ok(Wifi {
        blocking,
        _sub: sub,
    })
}

pub fn connect(wifi: &mut Wifi) -> Result<(), EspError> {
    let mut backoff = INITIAL_BACKOFF;
    let mut attempt = 0u32;

    loop {
        if wifi.blocking.is_up()? {
            return Ok(());
        }

        attempt += 1;
        match wifi.blocking.connect() {
            Ok(()) => {
                wifi.blocking.wait_netif_up()?;
                log::info!("WiFi connected (netif up)");
                return Ok(());
            }
            Err(e) => {
                log::warn!("WiFi connect attempt {attempt} failed: {e}; retrying in {backoff:?}");
                sleep(backoff);
                backoff = (backoff * 2).min(BACKOFF_CAP);
            }
        }
    }
}
