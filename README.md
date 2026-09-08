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
| `nix develop .#firmware-fhs` | Entorno del **firmware (FHS)**: igual que `firmware` pero en un sandbox FHS — **recomendado en NixOS** (no requiere nix-ld). |
| `nix develop .#firmware` | Entorno del **firmware**: rustup, espup, espflash, cargo-espflash, ldproxy, cargo-generate, libclang, cmake/ninja (en NixOS requiere nix-ld). |

### 7.1 Servidor

```sh
nix develop .#server
cd server
cargo run          # escucha en http://127.0.0.1:8080  (health en /health)
```

### 7.2 Firmware (ESP32)

> ⚠️ **`nix develop` (sin argumentos) entra al shell del SERVIDOR.** Para firmware usa siempre un
> shell de firmware (abajo). Si compilas firmware con el Rust estándar verás el error
> `could not create LLVM TargetMachine for triple: xtensa-none-elf` (el Rust estándar no soporta
> el backend Xtensa).

Hay dos formas de entrar al entorno de firmware. **En NixOS recomendamos `.#firmware-fhs`**
(entorno FHS), porque no requiere tocar la configuración del sistema.

#### Opción A — `.#firmware-fhs` (FHS, sin cambios al sistema)

Ejecuta el toolchain `esp` dentro de un sandbox con la jerarquía de Linux estándar
(`/lib64/ld-linux-x86-64.so.2`, `/usr/lib`, …), de modo que los binarios genéricos del
toolchain funcionan **sin `nix-ld`**.

```sh
nix develop .#firmware-fhs

# 1) verificar el toolchain (debe listar "esp")
rustup toolchain list      # debe listar "esp"
rustc +esp --version       # debe imprimir la versión (sin error stub-ld)

# 2) si "esp" no aparece, instalarlo (una única vez) y reentrar:
espup install
exit
nix develop .#firmware-fhs

# 3) compilar
cd firmware
cargo build                # target: xtensa-esp32-espidf
cargo espflash flash       # para flashear (con el ESP32 conectado)
```

#### Opción B — `.#firmware` (requiere `nix-ld` en NixOS)

Binarios del toolchain ejecutándose directamente en el sistema. En NixOS hay que habilitar
`nix-ld`:

```nix
programs.nix-ld.enable = true;
```

reconstruye con `sudo nixos-rebuild switch` y luego:

```sh
nix develop .#firmware
cd firmware && cargo build
```

El firmware ya está generado con los valores por defecto del template oficial esp-rs
(`MCU=esp32`, ESP-IDF `v5.5.3`, target `xtensa-esp32-espidf`).

## 8. Decisiones cerradas (hardware y alcance)

Cerradas con el hardware real del proyecto: **kit Keyestudio KS5024** (4WD Camera Robot
Car), ya probado con firmware C++ de referencia en
[ejverat/esp-cam-robot-car](https://github.com/ejverat/esp-cam-robot-car).

| Decisión | Resolución |
|----------|------------|
| **Placa y cámara** | **ESP32-CAM (AI-Thinker)** con **OV2640**. Pinout confirmado por el firmware C++ de referencia. |
| **Driver de motores** | **L298N** on-board, 2 canales PWM (LEDC): GPIO 12/13 (izq.) y 14/15 (der.). LED flash: GPIO 4. Sin encoder/IMU en el kit. |
| **Alcance de red** | **LAN primero** (fases 1–3, `ws://` sin auth); **acceso remoto después** → la fase 4 (WSS/TLS + auth por token) se mantiene en el roadmap. |
| **Video objetivo** | **MJPEG VGA (640×480), calidad JPEG 10, ~10–15 FPS, `fb_count=2` con PSRAM** — ya validado en el firmware C++. **WebRTC queda descartado** para este hardware. |
| **Agente IA** | Corre en el **servidor** (`ort`/`candle`), como cliente más del hub. El ESP32-CAM no tiene margen de cómputo para inferencia. |

**Implicaciones de diseño que quedan fijadas:**

- La cámara consume casi todos los GPIO; los 4 pines de motores + flash son lo disponible.
  No hay pines libres para IMU/encoders → la telemetría inicial es batería (ADC), RSSI WiFi
  y estado de motores.
- El esquema de control de motores replica el del firmware C++: `analogWrite` por pin
  (IN1/IN2 por lado), sin pin STBY.
- Resolución/FPS del stream se ajustan por `cmd:camera` dentro del rango que da PSRAM.
