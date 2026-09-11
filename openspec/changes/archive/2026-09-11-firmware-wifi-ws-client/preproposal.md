# Pre-proposal — firmware-wifi-ws-client

Estado pendiente de decisiones de producto antes de lanzar `sdd-proposal`.
Modo de ejecución: `auto`. Research: sin seleccionar (runtime sin evidencia documental/web).

## Decisión de producto pendiente (1) → RESUELTA (2026-09-09)

**c. NVS runtime + fallback** — SSID/password, URL del hub y `robot_id` se leen de NVS en
runtime; si la partición no tiene valores, se usan defaults compilados en `config.rs` y se
persisten en NVS en el primer arranque. Elegida por el usuario.

Opciones descartadas: a. hardcode solo en `config.rs`; b. Kconfig.

## Cuestiones técnicas (resueltas por proposal/design, no requieren decisión humana)

1. Semántica de registro: el hub auto-registra por query param (`?robot_id=`); no hace falta mensaje `register` del firmware.
2. El server no envía saludo ni JSON `ping` hoy → el firmware no puede asumir greeting ni ping aplicativo; keepalive a nivel protocolo WS (config explícita de `EspWebSocketClientConfig`) + TCP keepalive.
3. Fuente de `ts`: `None`/`0` por ahora (sin SNTP en este cambio; telemetría con ts llega en fase 3).
4. Watchdog/brownout: loop de reconexión WiFi con delays; no bloquear la task de FreeRTOS; brownout del kit (alimentación USB débil) queda documentado, sin tocar brownout detection en este cambio.
5. Transporte: `ws://` (LAN, fase 1); `wss://` en fase 4.
6. Dependencias: `esp_idf_svc::ws::client` requiere el extra component `espressif/esp_websocket_client` (v1.1.0) vía `[[package.metadata.esp-idf-sys.extra_components]]` en el mismo commit; `serde`/`serde_json` para el envelope.
7. `EspWebSocketClientConfig` con campos explícitos (el `Default` es todo ceros; silencia keepalive/backoff).
