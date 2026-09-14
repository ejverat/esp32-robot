# Apply Progress — firmware-motors

> Fase `sdd-apply`, backend `openspec`. Cambio: `firmware-motors`.
> Alcance de esta sesión: **Tareas 1–4 (código) + keep-out checklist**. La secuencia
> on-device (Phase A/B) queda **documentada** para el mantenedor; no se flasheó nada.

## Structured status consumed

- `changeName`: `firmware-motors` · `artifactStore`: `openspec` · `applyState`: `ready`
- `actionContext.mode`: `repo-local`; `allowedEditRoots`: `/home/ejverat/Projects/esp32-robot` (sin warnings)
- Review workload forecast: `Decision needed before apply: No`, `Chained PRs: No`,
  `400-line budget risk: Low`, `Delivery strategy: ask-on-risk`. Sin decisión de entrega pendiente.

## Resultado

Tareas 1–4 implementadas y verificadas por **cross-compile limpio** (`cargo build` exit 0) dentro
del FHS wrapper. Dos desviaciones mínimas del diseño (documentadas abajo), ambas con
comportamiento idéntico.

### Cross-compile exit codes

| Tarea | Comando | Exit |
|-------|---------|------|
| 1 — `command.rs` | `$(ls -d /nix/store/*-esp32-robot-firmware-fhs/bin/*firmware-fhs \| head -1) -c 'cd firmware && cargo build'` | **0** (7.35 s) |
| 2 — `motors.rs` | idem | **0** (5.89 s) — tras corregir imports `esp_idf_hal::` → `esp_idf_svc::hal::` |
| 3 — `ws_client.rs` + `main.rs` | idem | **0** — tras workaround del bug LLVM (ver desviación D2) + `cargo clean` + rebuild (1m55s) |
| 4 — `README.md` | N/A (solo docs) | — |

> Nota: el primer intento de compilar la Tarea 3 falló con un **bug de codegen LLVM**
> (`rustc-LLVM ERROR: Cannot select: XtensaISD::PCREL_WRAPPER TargetConstantPool [2 x float]
> [-1.0, 1.0]` en `serde_json::number::Number::deserialize_any`, LLVM 21.1.3 del toolchain `esp`
> rustc 1.97.0-nightly 2026-07-08). Ver desviación D2.

### Tamaño del `.bin` (AC12)

- `.bin` final: **1,385,088 bytes** (33.55 % de 4,128,768).
- Delta vs baseline anterior **1,273,008 bytes**: **+112,080 bytes** (~+112 KB).
- Dentro del rango previsto por el diseño §11.1 (~1.37–1.45 MB; serde_json dominante).

### Authored changed-line count (budget 400)

Excluye artefactos `openspec/` y lockfiles generados por el build; incluye `+` y `−`.

| Archivo | + | − |
|---------|---|---|
| `firmware/src/command.rs` (nuevo) | 145 | 0 |
| `firmware/src/motors.rs` (nuevo) | 148 | 0 |
| `firmware/src/ws_client.rs` | 15 | 0 |
| `firmware/src/main.rs` | 15 | 1 |
| `README.md` | 9 | 0 |
| **Total** | **332** | **1** |

**Total changed = 333 líneas** (bajo el presupuesto de 400; sin `size:exception`).
El exceso sobre el pronóstico de diseño (257) proviene de la desviación D2 (deserialización
manual de `MovePayload`, ~35 líneas) y de comentarios/blank-lines de los archivos nuevos.

## Files changed

- `firmware/src/command.rs` (nuevo): `DEADBAND`, `SideSpeeds`, `Cmd`, `CommandError`, `parse`, `mix`, `Envelope`, `MovePayload` (con `Deserialize` manual).
- `firmware/src/motors.rs` (nuevo): `MotorSignal`, `MotorSender`, `MAX_SPEED`/`INVERT_*`/`DEADMAN`, `Motors`, `init`, `apply`, `stop`, `run`.
- `firmware/src/ws_client.rs`: firma de `start()` con `motor_tx` + reenvío `Text`/`ConnectionLost` en el callback.
- `firmware/src/main.rs`: `mod command;`/`mod motors;`, canal `MotorSignal`, `motors::init` + `thread::spawn` **antes** de WiFi/WS, `motor_tx.clone()` a `ws_client::start`.
- `README.md`: nota en español (regla USB/batería, sin consola serie en batería, corrección por `INVERT_*`).

`firmware/Cargo.toml`, `firmware/Cargo.lock` y `firmware/sdkconfig.defaults` quedan **byte-identical** a HEAD.
`firmware/src/net.rs` y `firmware/src/wifi.rs` **sin cambios** (el bucle `rx.recv()` de `main` queda intacto).

## Desviaciones del diseño (comportamiento idéntico)

### D1 — `esp_idf_hal::ledc`/`esp_idf_hal::gpio` → `esp_idf_svc::hal::ledc`/`esp_idf_svc::hal::gpio`

El diseño §4.1 referencia `esp_idf_hal::ledc`. `esp-idf-hal` **no es dependencia directa** del crate
(solo transitiva); el crate se alcanza como re-export `esp_idf_svc::hal` (= `pub use esp_idf_hal::*`).
Mismos tipos (`LedcDriver`, `LedcTimerDriver`, `LowSpeed`, `LEDC`, `TimerConfig`, `Gpio12..15`), misma API.

### D2 — `MovePayload` deserializa `v`/`omega` como `f64` (impl manual) en vez de `#[derive(Deserialize)]` con `f32`

- **Causa raíz**: `#[derive(Deserialize)] struct MovePayload { v: f32, omega: f32 }` (diseño §7.1, congelado)
  dispara `serde_core::f32::deserialize` → `visit_f64` (`num_as_copysign_self!`, "Preserve sign of NaN"),
  cuyo `copysign` con `{1.0, -1.0}` genera un constant pool `[2 x float] [-1.0, 1.0]` que el backend Xtensa
  (LLVM 21.1.3) **no sabe seleccionar** (`PCREL_WRAPPER` ISel bug) → `cargo build` falla en codegen.
- **Workarounds descartados** (probados): `opt-level` 1/0 (el `#[inline]` instancia el código en
  `serde_json` a `opt-level="z"` igual); `default-features=false` en serde/serde_json (no surte efecto:
  `embedded-svc`/`esp-idf-svc` re-activan `serde_core/std`); downgrade de serde (bloqueado: `toml_datetime`
  exige `serde_core ^1.0.228`, que ya incluye `copysign`).
- **Adaptación mínima**: `MovePayload` mantiene el shape público `{ v: f32, omega: f32 }`; su `Deserialize`
  manual lee `v`/`omega` como `f64` (`visit_f64` de f64 = `num_self!`, sin `copysign`) y hace `as f32`.
  `parse()` conserva la validación `is_finite() || |v|>1.0 || |omega|>1.0` sobre los `f32` resultantes.
- **Comportamiento idéntico**: para valores finitos `as f32` da el mismo valor; `1e300` → `f32::INFINITY` →
  rechazado por `is_finite()` (igual que la ruta derive); payload no-objeto / campo erróneo / campo
  faltante → error → `WrongShape`; campos extra tolerados vía `IgnoredAny`. El signo de NaN (única
  diferencia del `copysign`) es irrelevante porque `parse` rechaza no-finitos.

## Verificación por inspección

- `command::mix` sigue el orden §5 exacto; ejemplos trabajados §5 verificados: recto `1.0/0.0 → 1.0/1.0`,
  giro izq `0.0/1.0 → −1.0/1.0`, normalización `1.0/0.5 → 0.333/1.0`, ambas en deadband → coast,
  omega≈0 → `0.8/0.8`. ✓
- `motors::apply` clampa a `[-MAX_SPEED, MAX_SPEED]` tras normalizar y antes del duty; `INVERT_*` niega el
  signo al final; duty = `((speed.abs() * get_max_duty() as f32).round() as u32).min(get_max_duty())`
  (nunca literal 255/256); encode §6 (`s>0` IN1=0/IN2=duty; `s<0` IN1=duty/IN2=0; `s=0` ambos 0). ✓
- `motors::run` = los 6 disparadores → un único `stop()`; `recv_timeout(DEADMAN=1000ms)`; `moving` solo se
  limpia en `Timeout` si `moving`. ✓
- Sin `unwrap`/`expect`/`panic!` sobre bytes del cable (solo `set_duty` con `if let Err = … { warn! }`). ✓
- `_timer` declarado **último** en `Motors`; `&timer` compartido en los 4 canales; sin `Rc`. ✓

## Keep-out checklist (verificado durante la revisión del diff)

- [x] **Cero** call-sites de salida `EspWebSocketClient::send*`. Los únicos `.send(` son canales mpsc
      internos (`NetEvent` en `wifi.rs`/`ws_client.rs`, `MotorSignal` en `ws_client.rs`). Sin `register`,
      `telemetry`, `pong`, `heartbeat`.
- [x] Sin código de cámara, telemetría, SNTP, TLS/WSS ni auth (solo `AuthMethod` WPA2 preexistente en `wifi.rs`).
- [x] Sin extensión de NVS/config para motores (solo constantes `MAX_SPEED`/`INVERT_*`/`DEADMAN`/`DEADBAND`).
- [x] Sin cambios en `server/`, web UI, partition-table ni brownout-detector.
- [x] `firmware/sdkconfig.defaults` byte-identical (0 diff).
- [x] Sin `embassy-executor` (solo `embassy-executor-timer-queue`, transitivo preexistente de `embassy-time`); `firmware/Cargo.toml` sin cambios.

## On-device verification (EJECUTADA — 2026-09-14, evidencia del mantenedor)

Hardware: ESP32-CAM + MB (CH340), L298N, 4WD, auto en bloques, alimentación por batería
(sin consola serie; hub `robot-hub.service` en `192.168.1.166:8080`). Comandos enviados
desde la estación con un cliente WS mínimo (conexión directa al hub).

### Resultados

1. **Fase A (banco USB)**: flash OK (`.bin` **1,385,088 bytes**, 33.55 %, +112,080 vs
   1,273,008); boot normal (`boot:0x13 SPI_FAST_FLASH_BOOT`), `config resolved`,
   `wifi:connected` (aid 26/28), `sta ip: 192.168.1.130`, **`WebSocket connected`**;
   robot `a1` en `/robots`. RX end-to-end probada con un `cmd:move v=5` inválido →
   `W invalid command InvalidValue; coasting` en serie (rechazo + coast).
2. **Sentidos (fase B)**: L1/L2 (solo izquierda) salían **invertidos** con el cableado
   de este kit; R1/R2 correctos → corrección aplicada: **`INVERT_LEFT = true`**
   (re-build + re-flash); re-validado: L1 adelante, L2 atrás, R1/R2 correctos.
3. **Dead-man**: un solo `cmd:move v=0.5` y silencio → parada a **~1 s** (cronometrada
   por el mantenedor).
4. **Corte de enlace**: stream continuo (comandos cada 0,25 s, dead-man siempre
   renovado) + corte del hub con marcador sonoro (`pw-play` de un beep generado *ad hoc*):
   las ruedas pararon **casi instantáneamente con el beep** (<1 s), lo que descarta al
   dead-man como causa (habría dado ~1 s extra) → camino `ConnectionLost → coast`
   validado; el robot reconectó solo (`a1`).

### Hallazgos de banco (no bloqueantes, para seguimiento)

- **Banda muerta de los motores izquierdos**: a duty ~30 % (`v=0.3`, 77/256) no arrancan;
  a 70 % (179/256) sí. Propuesta futura: piso mínimo de duty (solo si `|speed| > ε`).
- **Espaciado de comandos**: clientes lentos (>1 s entre `cmd:move`) producen pulsos
  (el dead-man corta entre comandos) — comportamiento esperado del diseño, documentado.
- **Técnica de banco**: para tests con cronometría subjetiva, usar un marcador sonoro
  en el instante del evento (`pw-play` + WAV generado con el `wave` de stdlib).

## Procedimiento de verificación on-device (documentado)

Wrapper: `WRAPPER="$(ls -d /nix/store/*-esp32-robot-firmware-fhs/bin/*firmware-fhs | head -1)"`.
**Regla dura: nunca USB + batería a la vez.**

### Phase A — banco USB (motores desconectados, o L298N sin alimentación de motores)

1. Build + flash: `$WRAPPER -c 'cd firmware && cargo build'` y
   `$WRAPPER -c 'cd firmware && espflash flash --port /dev/ttyUSB0 --baud 115200 target/xtensa-esp32-espidf/debug/esp32-robot-firmware'`.
   `.bin` medido: **1,385,088 bytes**. El chip queda en modo download → boot por RST o ciclo de USB.
2. Serial strap-safe: `$WRAPPER -c 'python .pi/skills/esp32-ondevice-workflow/assets/serial_capture.py --reset'`
   (suelta DTR/RTS, pulso RTS). Esperar `config resolved`, `wifi:connected`, `sta ip`, `WebSocket connected`.
3. Enviar un `cmd:move {v, omega}` vía hub (o cliente de prueba) y observar duty/signo izquierda/derecha logueado.
4. Verificar las tres paradas por software (motores sin carga):
   (a) **disconnect** (matar hub/cerrar cliente → `ConnectionLost` → stop);
   (b) **frame malformado** (no-JSON → `warn!` + coast);
   (c) **dead-man** (un `cmd:move` y silencio → stop ~1 s).
5. Opcional: scope GPIO12–15 para confirmar PWM 1 kHz y el encode forward/backward IN1/IN2.

### Phase B — batería (mantenedor, sin consola serie)

1. Flash por USB → **desconectar USB** → **conectar batería** (nunca ambos).
2. Observar físicamente (desde una estación LAN, no el robot): avance/retroceso/giro correctos y sentido `+v`=adelante, `+omega`=izquierda (CCW).
3. Observar **stop-on-disconnect** (cerrar cliente/matar hub → para) y **dead-man** (soltar controles → para ~1 s).
4. Confirmar robot en hub `/robots` (`curl http://<hub-ip>:8080/robots`).
5. Confirmar arranque normal con motores cableados (boot log no silencioso; strapping GPIO12/MTDI).
6. Si el avance físico va hacia atrás (o un lado invertido): mantenedor reporta qué lado; se voltea
   `INVERT_LEFT`/`INVERT_RIGHT` (`false → true`) y se re-flashea (nunca la convención).
7. **Report-back del mantenedor (evidencia requerida)**: (a) sentido avance/retroceso/giro vs esperado
   (o lado invertido); (b) stop-on-disconnect observado; (c) dead-man ~1 s observado; (d) robot en hub
   `/robots`; (e) boot normal con motores; (f) constantes `INVERT_*` volteadas, si aplica.

## Remaining tasks — estado final

- [x] Build + flash + registrar tamaño `.bin` (AC12): 1,385,088 bytes (33.55 %). <!-- sdd-owner: implementation -->
- [x] Captura serial strap-safe (`serial_capture.py --reset`): `config resolved`, `wifi:connected`, `sta ip`, `WebSocket connected`. <!-- sdd-owner: implementation -->
- [x] Enviar un `cmd:move {v, omega}` vía hub y observar duty/signo: rechazo de comando inválido + tests de aislamiento L/R. <!-- sdd-owner: implementation -->
- [x] Verificar las paradas por software: reject+coast (fase A), dead-man ~1 s (cronometrado), disconnect casi instantáneo (beep). <!-- sdd-owner: implementation -->
- [ ] Opcional: scope GPIO12–15 (PWM 1 kHz + encode IN1/IN2) — **no ejecutado** (sin osciloscopio). <!-- sdd-owner: implementation -->
- [x] Mantenedor: flash por USB → desconectar USB → conectar batería (nunca ambos). <!-- sdd-owner: implementation -->
- [x] Observar físicamente avance/retroceso/giro y sentido `+v`=adelante, `+omega`=izquierda (CCW). <!-- sdd-owner: implementation -->
- [x] Observar stop-on-disconnect y dead-man (~1 s). <!-- sdd-owner: implementation -->
- [x] Confirmar robot en hub `/robots`. <!-- sdd-owner: implementation -->
- [x] Confirmar boot normal con motores cableados (GPIO12/MTDI strapping). <!-- sdd-owner: implementation -->
- [x] Lado invertido detectado y corregido: `INVERT_LEFT = true` + re-flash + re-validación. <!-- sdd-owner: implementation -->
- [x] Report-back del mantenedor (sentidos, dead-man, corte, reconexión). <!-- sdd-owner: implementation -->

## Workload / PR boundary

Single PR, 4 work-unit commits (estilo convencional es): `Implementar comando (parse + mezcla)`,
`Implementar motores (LEDC + tarea)`, `Cablear WS + main (motores)`, `Docs: regla USB/batería y verificación`.
**333 líneas autorizadas** < 400; sin chaining, sin `size:exception`. No se commiteó (no solicitado).
