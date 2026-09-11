# Design — firmware-wifi-ws-client

> SDD Phase 1 slice: ESP32 firmware joins the LAN as a WiFi **station**, then opens a
> **WebSocket client** to the hub at `ws://<host>:<port>/ws/robot?robot_id=<id>`, with
> bounded-backoff WiFi reconnection, C-library WS auto-reconnect, and protocol-level
> keepalive.
>
> Backend: `openspec`. This document is the binding design for `sdd-tasks`.
> Proposal §4 decisions are binding and are NOT re-opened here.

Status: design (ready for `sdd-tasks`).

---

## 1. Overview

The firmware today is a hello-world (`link_patches()` + `EspLogger` + one `log::info!`).
This change adds four small modules that give the robot its network life, using only
blocking/callback APIs (no `embassy-executor`), matching the explore recommendation:

```
boot → config::resolve (NVS→defaults) → wifi::init (STA config + start + subscribe
StaDisconnected) → wifi::connect (bounded backoff) → ws_client::start (once,
auto-reconnect) → main loop blocks on rx.recv() (no busy-spin)
```

Non-goals (binding from proposal §5): motor/camera/telemetry/SNTP/TLS-WSS/auth/register
envelope/JSON heartbeat, brownout modification, static IP/mDNS. None are designed here.

---

## 2. Module layout and responsibilities

All under `firmware/src/`. Keep it small — this is a first slice, not a framework.

| File | Responsibility | Public items |
|------|----------------|--------------|
| `config.rs` (new) | Runtime config resolution in order **NVS → compiled defaults**; first-boot persistence; NVS keys + default consts. | `Config { ssid, password, hub_url, robot_id }`, `fn resolve(&EspNvs<EspDefaultNvs>) -> Result<Config, EspError>`, `const NVS_NAMESPACE`, key consts, default consts. |
| `net.rs` (new) | Shared event enum so `wifi`/`ws_client`/`main` don't depend on each other. | `enum NetEvent { WifiStaDisconnected, WsEvent }`, `type EventSender = std::sync::mpsc::Sender<NetEvent>`. |
| `wifi.rs` (new) | `BlockingWifi<EspWifi<'static>>` construction, STA config, `StaDisconnected` subscription, bounded-backoff connect loop. | `struct Wifi { blocking: BlockingWifi<EspWifi<'static>>, _sub: EspSubscription<'static, System> }`, `fn init(modem, &sysloop, nvs, tx) -> Result<Wifi, EspError>`, `fn connect(&mut Wifi) -> Result<(), EspError>`. |
| `ws_client.rs` (new) | Dial-URI construction (`host:port` parsing + `robot_id` URL-encoding), frozen `EspWebSocketClientConfig`, one-time client creation with callback → `tx`. | `fn build_uri(&Config) -> Result<String, UriError>`, `fn client_config() -> EspWebSocketClientConfig<'static>`, `fn start(uri, &config, tx) -> Result<EspWebSocketClient<'static>, EspIOError>`. |
| `main.rs` (rewrite) | Wire modules together; keep-alive loop blocking on `rx.recv()`. | `fn main() -> Result<(), EspError>`. |

**Data flow** (one shared `std::sync::mpsc::channel::<NetEvent>()`, unbounded — the upstream
`http_ws_client` example uses exactly this pattern):

- `wifi.rs` subscription → `tx.send(NetEvent::WifiStaDisconnected)` on `WifiEvent::StaDisconnected`.
- `ws_client.rs` callback → logs every event, then `tx.send(NetEvent::WsEvent)` (wake main only).
- `main` blocks on `rx.recv()`; on `WifiStaDisconnected` it re-runs `wifi::connect`; on `WsEvent`
  it does nothing (logging already happened in the callback).

The channel is **unbounded** and `Sender::send` is non-blocking, so it is safe to call from the
hidden WS task and the WiFi event-loop callback without risk of blocking either.

---

## 3. Frozen constants (spec open risks resolved)

### 3.1 NVS layout and first-boot flow

- Namespace: **`"robot"`** (single namespace). All key names ≤ 15 chars (NVS limit): verified.
- Keys and storage type (all `NVS_TYPE_STR`, via `EspNvs::get_str`/`set_str`):

| Key | Meaning | Read buffer | Default | Persisted on first boot |
|-----|---------|-------------|---------|--------------------------|
| `wifi.ssid` | SSID (≤ 32 B, WiFi driver limit) | `[u8; 33]` | `CHANGE_ME_SSID` | yes |
| `wifi.password` | PSK (≤ 64 B, WiFi driver limit) | `[u8; 65]` | `CHANGE_ME_PASSWORD` | yes |
| `hub.url` | hub address, single key `"host:port"` | `[u8; 64]` | `192.168.1.10:8080` | yes |
| `hub.robot_id` | robot identity | `[u8; 33]` | `a1` | yes |

Resolution per key (in `config::resolve`):

1. `get_str(key, &mut buf)`.
   - `Ok(Some(v))` with non-empty `v` → use it (NVS wins; default not consulted).
   - `Ok(None)` (absent) or empty string → use compiled default and `set_str(key, default)`
     (write-back), set `wrote_any = true`.
2. After the loop, if `wrote_any` → `log::info!("first boot: persisted defaults to NVS")`.
3. If any of `ssid`/`password`/`hub_url` equals its placeholder default →
   `log::warn!("running on placeholder defaults; provision real credentials in NVS")`.
4. **Never log the password.** SSID and `robot_id` may be logged; `hub.url` may be logged.

`EspDefaultNvsPartition::take()` is called **once** in `main` and **cloned** (the partition
type is `Clone`, backed by `Arc`): one clone goes to `EspNvs::new(..., "robot", true)` for
config, the other is passed to `EspWifi::new(..., Some(nvs))`. The `EspNvs` handle is dropped
after `resolve` returns (config is done); the partition clone stays alive inside `EspWifi`.

Malformed-but-present `hub.url` (see §3.2): do **not** overwrite NVS; log error and fall back to
the compiled default for that boot only.

### 3.2 `hub.url` single-key shape (unambiguous dial URI)

`hub.url` is exactly **one** key whose value is `"<host>:<port>"` — no scheme, no path, no
trailing slash. Examples: `192.168.1.10:8080`, `hub.lan:8080`, `[fd00::1]:8080` (IPv6 literal).

Parsing in `ws_client::build_uri`:

- `hub_url.rsplit_once(':')` → `(host, port_str)`. Splitting on the **last** colon keeps IPv6
  brackets intact and makes the port unambiguous.
- `port_str.parse::<u16>()` must succeed; `host` must be non-empty.
- Any failure → `UriError` → caller logs `error!` and falls back to `DEFAULT_HUB_URL`.

Dial URI (composed in code, never hand-written as a single string):

```
ws://<host>:<port>/ws/robot?robot_id=<percent-encode(robot_id)>
```

URL-encoding uses a private `fn percent_encode(&str) -> String` (RFC 3986: escape every byte
except `A–Z a–z 0–9 - . _ ~`, `%` + two uppercase hex). No new crate needed — this avoids
adding `percent-encoding`/`urlencoding` and keeps the dependency footprint exactly as specced.
`"a1"` passes through unchanged; any non-ASCII or reserved byte is escaped.

### 3.3 `EspWebSocketClientConfig` field values (frozen; all-zeros rejected)

The struct is `#[derive(Default)]` but the Rust `Default` yields **all-zeros**, which silently
disables keepalive/reconnect (proposal §4.6). Every required field is set **explicitly**; the
literal struct may use `..Default::default()` **only** for the optional/unused TLS/header fields,
never for any required field or `transport` (whose `Default` is the wrong `TransportUnknown`).

| Field (exact type) | Frozen value |
|--------------------|--------------|
| `transport` (`EspWebSocketTransport`) | `EspWebSocketTransport::TransportOverTCP` |
| `task_prio` (`u8`) | `5` |
| `task_stack` (`usize`) | `4096` |
| `buffer_size` (`usize`) | `1024` |
| `ping_interval_sec` (`Duration`, whole secs) | `Duration::from_secs(10)` |
| `pingpong_timeout_sec` (`Duration`, whole secs) | `Duration::from_secs(60)` (within spec's 60–120) |
| `reconnect_timeout_ms` (`Duration`, ms) | `Duration::from_millis(3000)` (within spec's 3000–5000) |
| `network_timeout_ms` (`Duration`, ms) | `Duration::from_millis(10000)` |
| `disable_auto_reconnect` (`bool`) | `false` (auto-reconnect ON) |
| `disable_pingpong_discon` (`bool`) | `false` (dead peers are disconnected) |
| send `timeout` arg to `EspWebSocketClient::new` (not a config field) | `Duration::from_secs(5)` |

No TLS fields are set (`server_cert`/`client_cert`/`client_key`/`use_global_ca_store`/CA-bundle
stay `None`/`false`). `user_agent`/`headers`/`username`/`password` stay `None`.

### 3.4 WiFi backoff constants (frozen)

| Constant | Value |
|----------|-------|
| initial backoff | `1s` |
| growth factor | `2` (1s → 2s → 4s → 8s → 15s) |
| cap | `15s` |
| reset | on successful connect (backoff is a local in `wifi::connect`; each invocation — including the post-disconnect re-entry — starts back at 1s) |

Upstream note: `BlockingWifi::connect()` already has an internal `CONNECT_TIMEOUT = 15s` and
blocks on the system event loop (not busy-wait), so a single attempt is self-bounded at ~15s
before our backoff sleep even runs.

---

## 4. Exact crate wiring (`firmware/Cargo.toml`)

Append (no other lines change; **no new Cargo feature** — the `wifi` and `ws::client` modules are
cfg-gated on default components, not Cargo features):

```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

and, at top level (end of file):

```toml
[[package.metadata.esp-idf-sys.extra_components]]
remote_component = { name = "espressif/esp_websocket_client", version = "1.1.0" }
```

- The `extra_components` entry is the **single required** dependency change; it satisfies the
  ESP-IDF ≥ 5 cfg gate `esp_idf_comp_espressif__esp_websocket_client_enabled` and makes
  `esp_idf_svc::ws::client` compile (confirmed in `esp-idf-svc 0.52.1 src/ws.rs`).
- `serde`/`serde_json` are **binding spec requirements** but are intentionally **unreferenced** in
  this slice (pre-provisioned for phase-3 telemetry / command parsing). Unused dependencies emit
  no default lint; this is accepted. Tasks must NOT remove them.
- `esp-idf-svc` features stay exactly `["critical-section", "embassy-time-driver", "embassy-sync"]`.

---

## 5. Control flow

### 5.1 `main()` (rewrite)

```
link_patches();
EspLogger::initialize_default();               // bind `log` to ESP log
Peripherals::take()?                           // modem consumed by wifi::init
EspSystemEventLoop::take()?                    // cloned into wifi + subscription
EspDefaultNvsPartition::take()?                // cloned: config + EspWifi
EspNvs::new(partition.clone(), "robot", true)? // read/write handle
config = config::resolve(&nvs)?;  drop(nvs);
(tx, rx) = mpsc::channel::<NetEvent>();
wifi  = wifi::init(modem, &sysloop, partition, tx.clone())?;  // sets config, starts, subscribes
wifi::connect(&mut wifi)?;                                     // bounded loop, returns when up
uri = ws_client::build_uri(&config)?;
_ws = ws_client::start(&uri, &ws_client::client_config(), tx.clone())?;  // created ONCE, never dropped
loop {
    match rx.recv() {
        NetEvent::WifiStaDisconnected => wifi::connect(&mut wifi)?,   // re-enter connect loop
        NetEvent::WsEvent => {},                                      // already logged in callback
        Err(_) => { warn!("event senders dropped; parking"); thread::park(); },
    }
}
```

`main` returns `Result<(), EspError>`. Boot-setup failures (logger, peripherals, event loop, NVS,
config read, wifi init) are unrecoverable and return `Err` (logged at `error!`; the device halts
— a reset supervisor is explicitly out of scope). Everything after wifi init is recoverable and
loops.

### 5.2 WiFi connect loop (`wifi::connect`)

`wifi::init` does, once: build `EspWifi::new(modem, sysloop.clone(), Some(nvs))`, wrap in
`BlockingWifi`, `set_configuration(&Client(ClientConfiguration{..}))`, `start()`, and
`sysloop.subscribe::<WifiEvent, _>(|ev| if StaDisconnected => tx.send(WifiStaDisconnected))`
(the returned `EspSubscription<'static, System>` is stored in `Wifi::_sub` and kept alive).

`wifi::connect` is a bounded loop:

```
loop {
    if wifi.blocking.is_up()? { return Ok(()); }        // early-out if already connected
    match wifi.blocking.connect() {                     // blocks ≤15s on event loop
        Ok(()) => { wifi.blocking.wait_netif_up()?; return Ok(()); }
        Err(e) => { warn!(attempt, e, backoff); sleep(backoff); backoff = (backoff*2).min(15s); }
    }
}
```

`set_configuration`/`start` are NOT repeated on re-entry (STA stays "started" across disconnects);
`connect()` re-issues `esp_wifi_connect`. The `ClientConfiguration` is built from `Config`
(ssid/password via `try_into::<heapless::String<32/64>>()`, `auth_method` pinned to
`AuthMethod::WPA2Personal` as a named `const`, `bssid`/`channel` `None`, rest `Default`).

### 5.3 WS client — create once, auto-reconnect

`ws_client::start` calls `EspWebSocketClient::new(uri, &cfg, 5s, move |event| { log; tx.send(WsEvent) })`
and returns immediately (the C library starts a hidden task; it does **not** block waiting for a
greeting). The client is created **once**, after `wait_netif_up()`, and `_ws` keeps it alive for
the whole program (the infinite loop never drops it).

Reconnection: the C library owns WS-level reconnection (`disable_auto_reconnect=false`,
`reconnect_timeout_ms=3s`). On hub restart / half-open drop it fires `Disconnected` → (auto) →
`Connected`, all logged. The firmware never recreates the client and never sends manual Ping/Pong
(`EspWebSocketClient::send` **panics** on `FrameType::Ping/Pong` — confirmed in the vendored
source; keepalive is config-driven only).

### 5.4 Event handling (WS callback)

| Event | Action |
|-------|--------|
| `BeforeConnect` | `debug!` |
| `Connected` | `info!("WebSocket connected")` |
| `Disconnected` | `info!("WebSocket disconnected (auto-reconnect pending)")` |
| `Close(reason)` | `info!("WebSocket close, reason: {reason:?}")` |
| `Closed` | `info!("WebSocket closed")` |
| `Text`/`Binary`/`Ping`/`Pong` | `debug!` (nothing sent; inbound ignored this slice) |
| `Err(_)` (`WEBSOCKET_EVENT_ERROR`) | `warn!("WebSocket error: {e}")` |

After logging, the callback sends `NetEvent::WsEvent` (best-effort `.ok()`).

---

## 6. Error handling and logging policy

- Log levels: **Info** default (no `set_max_level` override needed; `Debug` optional for bring-up).
- `error!` — unrecoverable boot failures (return `Err` and halt).
- `warn!` — each failed WiFi connect attempt (with attempt # and next backoff), WS `Err` events,
  placeholder-defaults-in-use, malformed `hub.url`.
- `info!` — WiFi started/connected/netif-up, `StaDisconnected`, WS `Connected`/`Disconnected`/
  `Close`/`Closed`, first-boot NVS persistence.
- `debug!` — WS `BeforeConnect`/`Text`/`Binary`/`Ping`/`Pong`.
- **Never log `wifi.password`.** SSID / `robot_id` / `hub.url` are safe to log.
- No panics in the recoverable paths; `send().ok()` on the channel; `std::thread::park()` on the
  (should-not-happen) all-senders-dropped case.

---

## 7. Verification plan (firmware has no host tests — `openspec/config.yaml`)

`strict_tdd` does not apply to firmware-only code. Each requirement maps to concrete evidence:

| Requirement | Verification |
|-------------|--------------|
| WiFi + NVS resolve/persist | Cross-compile (`nix develop .#firmware-fhs` + `cd firmware && cargo build`) proves NVS+wifi APIs resolve; inspection of `config.rs` (read → fallback → `set_str` write-back); on-device serial log shows connect to expected SSID and "persisted defaults" on first boot. |
| Layered config (NVS wins) | Inspection of `config::resolve` order and key constants; cross-compile. |
| WS dial URI + URL-encode + no greeting wait | Inspection of `build_uri`/`percent_encode`/`TransportOverTCP`; cross-compile; on-device hub log shows the robot registered via the query string; firmware treats `Connected` as success (no receive call). |
| Bounded backoff + STA re-entry + WS auto-reconnect | Inspection (backoff cap/factor, `disable_auto_reconnect=false`, client created once); on-device: kill hub / drop WiFi, observe reconnection, no TWDT reset. |
| Explicit non-zero config | Inspection of `client_config()` — each required field assigned a non-zero literal; `..Default::default()` allowed only for optional TLS/header fields. |
| Logging transitions | Inspection of log call sites; on-device serial monitor. |
| Deps + cross-compile | Run the build; inspect Cargo.toml for serde/serde_json + `esp_websocket_client` 1.1.0 entry. |
| Main stays alive, no busy-spin | Inspection: loop blocks on `rx.recv()` (not a spin poll); `thread::sleep`/event-loop waits for backoff/connect. |
| No register/heartbeat/ts | Inspection: zero `client.send(...)` call sites in this slice; `ts` never constructed. |
| Non-goals + sdkconfig/partition stable | Diff review: no motor/camera/telemetry/SNTP/TLS/auth modules; `sdkconfig.defaults` byte-identical; no partition table introduced. |

On-device sequence (documented in tasks/verify): flash with `cd firmware && cargo espflash flash`,
observe serial for WiFi→Connected→WS Connected; restart the hub to see Disconnected→Connected;
`erase-flash` clears NVS for a first-boot re-test.

---

## 8. Rollback / keep-out

- Revert commit + re-flash previous firmware. No server, schema, or partition-table changes.
- **`sdkconfig.defaults`: unchanged (0 diff).** `esp_wifi`/`nvs_flash`/`esp_timer`/`esp_event`
  are on by default for `MCU=esp32`, and the default single-app partition table already includes
  an `nvs` partition (explore §1.3 verified). The WS component is injected via Cargo metadata,
  not Kconfig. The optional `CONFIG_ESP_WIFI_ENABLED=y` is **not** added (redundant; keeps a zero
  sdkconfig diff).
- Brownout detector: **untouched** (documented risk, pre-proposal decision).
- NVS keys are additive; cleared with `erase-flash` if ever needed.

---

## 9. Risks

| Risk | Mitigation |
|------|------------|
| All-zeros WS config silently disables keepalive/reconnect | §3.3 frozen values; every required field + `transport` assigned explicit literals. |
| Brownout reset on ESP32-CAM WiFi TX spikes (weak USB) | Documented (README, es), NOT modified. Adequate power/short cable before flashing. |
| TWDT on long blocking connects | `BlockingWifi::connect` self-bounds at 15s + event-loop wait (not busy-wait); backoff `thread::sleep` yields to FreeRTOS. If TWDT still fires on-device, a scoped follow-up (feed/disable main-task watchdog) is documented, not silently added. |
| Binary size (serde_json + WS component) | Profiles already `opt-level = "z"`/`"s"`; check `.bin` size during apply. |
| Stale/half-open connection | Protocol ping (`ping_interval_sec=10`, `pingpong_timeout_sec=60`), `disable_pingpong_discon=false`. |
| Secrets in repo | NVS + placeholder defaults only; no real credentials committed. |
| Review-budget (see §10) | Borderline vs 400-line budget → exact count in tasks; ask-on-risk pause if >400. |
| `hub.url` malformed | `error!` + fall back to `DEFAULT_HUB_URL` for that boot, do not overwrite NVS. |

---

## 10. Expected diff size (feeds `sdd-tasks` review-budget forecast)

| File | Kind | Est. changed lines |
|------|------|--------------------|
| `firmware/Cargo.toml` | edit | +8 |
| `firmware/src/main.rs` | rewrite (~11 → ~70) | ~60 |
| `firmware/src/config.rs` | new | ~85 |
| `firmware/src/net.rs` | new | ~10 |
| `firmware/src/wifi.rs` | new | ~100 |
| `firmware/src/ws_client.rs` | new | ~100 |
| `firmware/sdkconfig.defaults` | unchanged | 0 |
| `README.md` firmware note (Spanish, `user_facing_docs: es`) | edit | ~12 |
| **Total** | | **≈ 375 (range 360–420)** |

The forecast straddles the **400-line review budget**. Delivery strategy is `ask-on-risk`, so
`sdd-tasks` MUST produce an exact line count; if it exceeds 400 changed lines, pause and ask
rather than auto-chain or infer an exception. `exception-ok` is `false` (needs explicit human
acceptance). Small modules were chosen specifically to keep this under budget; no chaining is
planned.

---

## 11. Decisions to carry into `sdd-tasks`

1. Implement in work-unit commits: (a) Cargo.toml deps + component, (b) `config.rs` + `net.rs`,
   (c) `wifi.rs`, (d) `ws_client.rs`, (e) `main.rs` rewrite + README note — each cross-compiling.
2. `serde`/`serde_json` are declared-but-unused this slice (binding spec); do not drop them.
3. `AuthMethod::WPA2Personal` is a named `const` in `wifi.rs` (changeable without touching logic).
4. `EspWebSocketClientConfig` literal uses `..Default::default()` only for optional/unused fields.
5. README note is in Spanish; covers NVS credentials, `erase-flash`, brownout/power caveat.
