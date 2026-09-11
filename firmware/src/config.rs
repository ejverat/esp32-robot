//! Resolución de configuración en tiempo de ejecución: NVS → valores por defecto.
//! En el primer arranque, los valores por defecto se persisten de vuelta en NVS.

use esp_idf_svc::nvs::EspDefaultNvs;
use esp_idf_svc::sys::EspError;

pub const NVS_NAMESPACE: &str = "robot";
pub const KEY_SSID: &str = "wifi.ssid";
pub const KEY_PASSWORD: &str = "wifi.password";
pub const KEY_HUB_URL: &str = "hub.url";
pub const KEY_ROBOT_ID: &str = "hub.robot_id";

pub const DEFAULT_SSID: &str = "CHANGE_ME_SSID";
pub const DEFAULT_PASSWORD: &str = "CHANGE_ME_PASSWORD";
pub const DEFAULT_HUB_URL: &str = "192.168.1.10:8080";
pub const DEFAULT_ROBOT_ID: &str = "a1";

pub struct Config {
    pub ssid: String,
    pub password: String,
    pub hub_url: String,
    pub robot_id: String,
}

pub fn resolve(nvs: &EspDefaultNvs) -> Result<Config, EspError> {
    let mut wrote_any = false;

    let mut ssid_buf = [0u8; 33];
    let mut password_buf = [0u8; 65];
    let mut hub_url_buf = [0u8; 64];
    let mut robot_id_buf = [0u8; 33];

    let ssid = read_or_default(nvs, KEY_SSID, &mut ssid_buf, DEFAULT_SSID, &mut wrote_any)?;
    let password =
        read_or_default(nvs, KEY_PASSWORD, &mut password_buf, DEFAULT_PASSWORD, &mut wrote_any)?;
    let hub_url =
        read_or_default(nvs, KEY_HUB_URL, &mut hub_url_buf, DEFAULT_HUB_URL, &mut wrote_any)?;
    let robot_id =
        read_or_default(nvs, KEY_ROBOT_ID, &mut robot_id_buf, DEFAULT_ROBOT_ID, &mut wrote_any)?;

    if wrote_any {
        log::info!("first boot: persisted defaults to NVS");
    }

    if ssid == DEFAULT_SSID || password == DEFAULT_PASSWORD || hub_url == DEFAULT_HUB_URL {
        log::warn!("running on placeholder defaults; provision real credentials in NVS");
    }

    log::info!("config resolved: ssid={ssid}, hub_url={hub_url}, robot_id={robot_id}");

    Ok(Config {
        ssid,
        password,
        hub_url,
        robot_id,
    })
}

fn read_or_default(
    nvs: &EspDefaultNvs,
    key: &str,
    buf: &mut [u8],
    default: &str,
    wrote_any: &mut bool,
) -> Result<String, EspError> {
    match nvs.get_str(key, buf)? {
        Some(value) if !value.is_empty() => Ok(value.to_string()),
        _ => {
            nvs.set_str(key, default)?;
            *wrote_any = true;
            Ok(default.to_string())
        }
    }
}
