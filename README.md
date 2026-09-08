# ESP32 Robot — Monitoreo, control y conducción autónoma (Rust)

> **Estado:** Propuesta inicial — documento vivo, sujeto a correcciones y adecuaciones.

## 1. Descripción del proyecto

Proyecto para controlar un **auto/robot** equipado con un **ESP32** y una **cámara**, con dos
objetivos principales:

1. **Control remoto y monitoreo** desde una **página web** (teléfono o PC): ver el video de la
   cámara en tiempo real, enviar comandos de movimiento y recibir telemetría del robot
   (batería, velocidad, orientación, calidad de señal WiFi, etc.).
2. **Conducción autónoma (fase futura):** conectar un **agente de inteligencia artificial** que
   consuma el video, **identifique objetos** (personas, obstáculos, señales, etc.) y decida los
   movimientos del robot de forma independiente.

La intención es usar el lenguaje **Rust en todos los componentes posibles**, tanto en el
firmware del ESP32 como en el servidor, el panel web y el futuro agente de IA.

## 2. Objetivos

- **Teleoperación** en tiempo real desde el navegador (comandos de movimiento).
- **Streaming de video** de baja latencia desde la cámara del robot.
- **Telemetría** del estado del robot en vivo.
- **Flota escalable**: controlar varios robots desde la misma interfaz (identidad `robot_id`).
- **Base preparada para IA**: que el agente pueda suscribirse al mismo flujo de video/telemetría
  y publicar comandos de movimiento sin cambiar el transporte.
- **Rust como lenguaje principal** en firmware, backend, frontend y agente.

## 3. Recomendación resumida (TL;DR)

- **Arquitectura:** modelo **hub** con un **servidor central en Rust (Axum + Tokio)**. El ESP32 se
  comporta como **cliente WebSocket** del servidor; el navegador también se conecta al servidor.
- **Protocolo de transporte:** **WebSocket** (bidireccional, nativo en navegadores y disponible en
  `esp-idf-svc`). Opcionalmente **WebRTC** a futuro para video de muy baja latencia.
- **Video:** **MJPEG** (frames JPEG) sobre WebSocket en la fase 1.
- **Control/telemetría:** mensajes **JSON versionados** (evolución posterior a CBOR/MessagePack).
- **IA (futuro):** corre en el servidor Rust (`ort` para ONNX, o `candle`), consumiendo los mismos
  frames y publicando comandos.

El detalle completo, con diagramas, alternativas y justificación, está en
[`docs/architecture.md`](docs/architecture.md).

## 4. Estructura propuesta del repositorio

```
esp32-robot/
├── flake.nix                  # Dev environments: .#server y .#firmware
├── README.md                  # Descripción general (este archivo)
├── docs/
│   └── architecture.md        # Arquitectura, protocolos y diagramas
├── server/                    # Servidor central Rust (Axum + Tokio)
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
├── firmware/                  # Rust para ESP32 (esp-rs / esp-idf)
│   ├── Cargo.toml
│   ├── build.rs
│   ├── rust-toolchain.toml    # channel = "esp"
│   ├── sdkconfig.defaults
│   ├── .cargo/
│   │   └── config.toml        # target xtensa-esp32-espidf
│   └── src/
│       └── main.rs
├── web/                       # (futuro) Panel web Rust → WASM
└── ai/                        # (futuro) Agente IA: detección + planificación
```

> `server/` y `firmware/` ya son crates funcionales (hola-mundo). `web/` y `ai/` se agregarán
> en fases posteriores.

## 5. Stack tecnológico (Rust)

| Componente | Tecnología sugerida | Notas |
|------------|---------------------|-------|
| Firmware ESP32 | `esp-rs` + `esp-idf-svc` / `esp-idf-hal`, `esp32-camera` | Usar el runtime de ESP-IDF facilita WiFi, cámara y WebSocket en Rust. |
| Servidor central | `axum` + `tokio` + `tokio-tungstenite` | Hub WebSocket, relay de video/telemetría, TLS con `rustls`. |
| Panel web | `leptos` / `yew` / `dioxus` + `trunk` (WASM) | UI en Rust compilada a WASM. |
| Agente IA | `ort` (ONNX Runtime) o `candle` (Hugging Face) | YOLO/SSD/MobileNet para detección de objetos. |
| Serialización | `serde` + `serde_json` (fase 1) → `serde_cbor`/`rmp` (fase 2) | Mensajes versionados. |

## 6. Cómo seguir

1. Leer [`docs/architecture.md`](docs/architecture.md) para validar la propuesta.
2. Configurar los entornos de desarrollo (sección 7).
3. Confirmar decisiones abiertas (sección 8).
4. Implementar el MVP: WebSocket + MJPEG + comandos de movimiento.

## 7. Entornos de desarrollo (Nix flakes)

Cada componente tiene su *dev environment* definido en el `flake.nix` raíz. Requisito: Nix con
`flakes` habilitados.

| Comando | Qué hace |
|---------|----------|
| `nix develop` (o `nix develop .#server`) | Entorno del **servidor**: Rust (cargo, rustc, clippy, rust-analyzer), cargo-watch, openssl. |
| `nix develop .#firmware` | Entorno del **firmware**: rustup, espup, espflash, cargo-espflash, ldproxy, cargo-generate, libclang, cmake/ninja. |

### 7.1 Servidor

```sh
nix develop .#server
cd server
cargo run          # escucha en http://127.0.0.1:8080  (health en /health)
```

### 7.2 Firmware (ESP32)

El toolchain de Rust para Xtensa se instala **una sola vez** con `espup`:

```sh
nix develop .#firmware
espup install              # descarga el toolchain esp (una única vez)
# salir y volver a entrar para cargar ~/export-esp.sh
cd firmware
cargo build                # target: xtensa-esp32-espidf
cargo espflash flash       # para flashear (con el ESP32 conectado)
```

El firmware ya está generado con los valores por defecto del template oficial esp-rs
(`MCU=esp32`, ESP-IDF `v5.5.3`, target `xtensa-esp32-espidf`).

## 8. Decisiones abiertas / pendientes de validar

- Modelo exacto de ESP32 y cámara (¿ESP32-CAM? ¿ESP32 + OV2640? ¿resolución objetivo?).
- Driver de motores y alimentación (afecta al esquema de control PWM).
- Si se requiere acceso desde fuera de la red local (implica TLS + autenticación).
- Target de latencia y FPS del video (define si MJPEG basta o hace falta WebRTC).
- Dónde correrá la IA: en el servidor (recomendado) vs. dispositivo externo.

Estas decisiones se cerrarán en la próxima iteración de este documento.
