# firmware-network Specification

> Change: `firmware-wifi-ws-client`. New domain (no canonical spec exists yet).
> This spec describes WHAT must be true after the firmware gains its network life:
> WiFi station connection + WebSocket client to the hub, with bounded reconnection
> and protocol-level keepalive. Implementation details (exact struct names, loop
> shape) belong to the design/tasks phases.

## Purpose

Give the ESP32 firmware a network transport to the hub: join the LAN as a WiFi
**station** using credentials resolved from NVS (with compiled-default fallback),
then open a **WebSocket client** to `ws://<host>:<port>/ws/robot?robot_id=<id>`,
stay connected across WiFi drops and hub restarts via bounded backoff and explicit
protocol keepalive, and log all connection state transitions. This is the smallest
slice that makes the robot present on the network; no application messaging,
telemetry, motors, or camera is in scope.

## Requirements

### Requirement: WiFi station connection with NVS-resolved credentials

The firmware MUST connect to the configured WiFi network as a station. It MUST
resolve the SSID and password from NVS at runtime. When the relevant NVS keys are
absent or empty (first boot), the firmware MUST fall back to the compiled defaults
in `config.rs` and MUST persist those defaults to NVS so later boots read them back.

> Verification: cross-compilation (NVS + wifi APIs resolve) + code inspection
> (read → fallback → write-back path) + on-device manual check (serial log shows a
> successful station connect to the expected SSID).

#### Scenario: First boot with empty NVS

- GIVEN NVS has no `wifi.ssid` and no `wifi.password` entries
- WHEN the firmware boots
- THEN it uses the compiled default credentials, persists them to NVS, and attempts a station connection with those credentials

#### Scenario: Subsequent boot with credentials present

- GIVEN NVS already contains `wifi.ssid` and `wifi.password`
- WHEN the firmware boots
- THEN it reads those NVS values and connects with them, ignoring the compiled defaults

#### Scenario: Credential storage keeps secrets out of source

- GIVEN the source tree is inspected
- WHEN reviewing `config.rs` and `main.rs`
- THEN no real SSID/password secrets are committed; only placeholder defaults are present

### Requirement: Layered configuration resolution

The firmware MUST include a `config.rs` module that resolves runtime configuration
in order **NVS → compiled defaults**. It MUST cover at minimum: SSID, password, hub
URL (host + port), and `robot_id`. The default `robot_id` MUST be `"a1"` (matching
the server tests).

> Verification: code inspection (resolution order and keys) + cross-compilation.

#### Scenario: NVS value wins

- GIVEN NVS contains a `hub.robot_id` value
- WHEN configuration is resolved
- THEN the NVS value is used and the compiled default is not consulted

#### Scenario: Compiled default used when NVS empty

- GIVEN NVS is empty for a configuration key
- WHEN configuration is resolved
- THEN the compiled `config.rs` default is used (and persisted on first boot, per the WiFi requirement)

#### Scenario: Default robot identity

- GIVEN no `hub.robot_id` override exists
- WHEN configuration is resolved
- THEN `robot_id` defaults to `"a1"`

### Requirement: WebSocket client connection to the hub

The firmware MUST open a WebSocket client to `ws://<host>:<port>/ws/robot?robot_id=<id>`,
URL-encoding the `robot_id` query value. The transport MUST be `TransportOverTCP`
(`ws://` only; no TLS fields configured). Connection success MUST be determined by the
WebSocket `Connected` event — the firmware MUST NOT block waiting for a server greeting
(the server sends nothing on connect today).

> Verification: code inspection (URI construction, URL encoding, transport enum) +
> cross-compilation + on-device manual check (hub log shows the robot registered via
> the query string).

#### Scenario: Correct dial URI

- GIVEN hub URL `host:port` and `robot_id` from configuration
- WHEN the WebSocket client is created
- THEN it dials `ws://<host>:<port>/ws/robot?robot_id=<url-encoded-id>`

#### Scenario: Handshake via query param, no greeting wait

- GIVEN the firmware connects to the hub
- WHEN the TCP + WebSocket upgrade completes
- THEN the hub auto-registers the robot from the `robot_id` query param and the firmware treats `Connected` as success without waiting for any server message

#### Scenario: No TLS in this slice

- GIVEN the transport configuration is inspected
- WHEN the client is built
- THEN `TransportOverTCP` is used and no TLS/certificate fields are set

### Requirement: Bounded backoff reconnection without indefinite blocking

The firmware MUST reconnect after disconnection without an infinite tight loop and
without blocking the FreeRTOS task indefinitely. WiFi connect retries MUST use a
bounded exponential backoff with a documented cap (e.g. 1s → 2s → 4s, capped at
15s). On `STA_DISCONNECTED`, the firmware MUST re-enter the WiFi connect loop. The
WebSocket client MUST be created once with auto-reconnect enabled
(`disable_auto_reconnect = false`) so the C library handles WS-level reconnection.

> Verification: code inspection (backoff bounds, cap, one-time client creation,
> `disable_auto_reconnect = false`) + on-device manual check (kill the hub / drop
> WiFi and observe reconnection without a watchdog reset).

#### Scenario: WiFi connect failure retries with bounded backoff

- GIVEN the configured AP is not reachable
- WHEN a connect attempt fails
- THEN the firmware logs the attempt, waits a bounded backoff interval, and retries, with the delay never exceeding the documented cap

#### Scenario: Post-connection disconnect re-enters connect loop

- GIVEN the firmware is already connected as a station
- WHEN `STA_DISCONNECTED` is observed
- THEN the firmware re-runs the WiFi connect loop

#### Scenario: WS auto-reconnect on hub restart

- GIVEN a connected WebSocket client
- WHEN the hub drops the connection
- THEN the C library reconnects automatically (client created once, `disable_auto_reconnect = false`) and the firmware logs the disconnect/reconnect transitions

#### Scenario: No indefinite blocking during connect/backoff

- GIVEN a long or repeated connect/backoff sequence
- WHEN the firmware runs
- THEN no single connect/backoff step blocks the FreeRTOS task indefinitely (task watchdog not tripped)

### Requirement: Explicit non-zero keepalive and client configuration

The WebSocket client config MUST set explicit, non-zero values for `task_prio`,
`task_stack`, `buffer_size`, `ping_interval_sec`, `pingpong_timeout_sec`,
`reconnect_timeout_ms`, and `network_timeout_ms`, MUST set
`transport = TransportOverTCP`, and MUST keep `disable_auto_reconnect = false`.
The all-zeros `Default` config (which silently disables keepalive and auto-reconnect)
MUST NOT be used.

> Verification: code inspection (each config field assigned a non-zero value; no
> reliance on `Default` for the required fields) + cross-compilation.

#### Scenario: Keepalive interval and timeout are set

- GIVEN the WebSocket client config is inspected
- WHEN the firmware builds the client
- THEN `ping_interval_sec` is non-zero (e.g. 10) and `pingpong_timeout_sec` is a non-zero bounded value (e.g. 60–120)

#### Scenario: Reconnect/network timeouts are set

- GIVEN the WebSocket client config is inspected
- WHEN the firmware builds the client
- THEN `reconnect_timeout_ms` (e.g. 3000–5000) and `network_timeout_ms` (e.g. 10000) are non-zero

#### Scenario: Task resources and buffer size are set

- GIVEN the WebSocket client config is inspected
- WHEN the firmware builds the client
- THEN `task_prio` (e.g. 5), `task_stack` (e.g. 4096), and `buffer_size` (e.g. 1024) are non-zero

### Requirement: Connection state transitions are logged

The firmware MUST log WiFi and WebSocket connection state transitions via the ESP log
facade (the `log` crate bound to `EspLogger`). At minimum, each WiFi connect attempt,
WiFi connect/disconnect, and WS `Connected` / `Disconnected` / `Close` event MUST be
logged.

> Verification: code inspection (log calls in the WiFi loop and WS callback) +
> on-device manual check (serial monitor shows the transition logs).

#### Scenario: WiFi attempt and result logged

- GIVEN the firmware is attempting a WiFi connection
- WHEN an attempt starts, succeeds, or fails
- THEN a log line describing the attempt/result is emitted via `log`

#### Scenario: WS lifecycle logged

- GIVEN the WebSocket client is running
- WHEN `Connected`, `Disconnected`, or `Close` events fire
- THEN each transition is logged via `log`

### Requirement: Dependency and build contract

`firmware/Cargo.toml` MUST add `serde` and `serde_json` as dependencies, and MUST add a
`[[package.metadata.esp-idf-sys.extra_components]]` entry with
`remote_component = { name = "espressif/esp_websocket_client", version = "1.1.0" }`.
The firmware MUST cross-compile with `nix develop .#firmware-fhs` and
`cd firmware && cargo build` (target `xtensa-esp32-espidf`). No new Cargo feature is
required for the WiFi module.

> Verification: cross-compilation (build succeeds) + code inspection (Cargo.toml
> entries present with the specified version).

#### Scenario: WebSocket component is injected

- GIVEN `firmware/Cargo.toml` is inspected
- WHEN the build runs
- THEN the `espressif/esp_websocket_client` (1.1.0) managed component is fetched via the `extra_components` entry and the `ws::client` module is available

#### Scenario: JSON dependencies present

- GIVEN `firmware/Cargo.toml` is inspected
- WHEN the firmware builds
- THEN `serde` (with `derive`) and `serde_json` are available dependencies

#### Scenario: Firmware cross-compiles

- GIVEN `nix develop .#firmware-fhs` is active
- WHEN `cd firmware && cargo build` is run
- THEN the firmware compiles for `xtensa-esp32-espidf` without errors

### Requirement: Main task stays alive without busy-spinning

`main()` MUST keep the firmware alive by blocking on the WebSocket callback channel
(e.g. `loop { rx.recv() }` over an `std::sync::mpsc` channel fed by the WS callback)
and MUST NOT busy-spin.

> Verification: code inspection (main loop blocks on a channel receive, not a
> spinning poll).

#### Scenario: Event-driven idle

- GIVEN no WebSocket events are pending
- WHEN `main()` reaches its keep-alive loop
- THEN it blocks on the channel receive (no busy-spin)

### Requirement: No register envelope, no application heartbeat, ts omitted

The firmware MUST NOT emit a `register` envelope (the hub auto-registers from the
query string) and MUST NOT emit an application-level JSON heartbeat; keepalive is
protocol-level WS ping/pong only. When future messages are emitted, `ts` MUST be
omitted or `0` (no SNTP in this change).

> Verification: code inspection (no `register`/JSON `ping` send call sites).

#### Scenario: Nothing sent on connect

- GIVEN the WebSocket connection is established
- WHEN the firmware runs
- THEN it sends no `register` envelope and no JSON ping/pong message

### Requirement: Non-goals and build-config stability

The change MUST NOT add motor, camera, telemetry, SNTP, TLS/WSS, or auth code paths.
The partition table MUST NOT change, and `sdkconfig.defaults` MUST remain functionally
unchanged (only an optional explicit `CONFIG_ESP_WIFI_ENABLED=y` for legibility is
allowed).

> Verification: code inspection / diff review (no new modules or call sites for the
> excluded areas; no partition-table or functional sdkconfig changes).

#### Scenario: Excluded subsystems untouched

- GIVEN the change diff is reviewed
- WHEN checking the firmware source
- THEN no motor/camera/telemetry/SNTP/TLS/auth code is present

#### Scenario: Partition and sdkconfig stable

- GIVEN `sdkconfig.defaults` and the partition table are inspected
- WHEN the change is applied
- THEN the partition table is unchanged and `sdkconfig.defaults` has no functional change
