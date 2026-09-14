# Pre-proposal — firmware-motors

Decisiones de producto resueltas y congeladas (ver abajo); el proposal las hereda verbatim.
Modo de ejecución: `auto`. Research: sin seleccionar (runtime sin evidencia documental/web).

## Decisiones de producto (7) → RESUELTAS (2026-09-11)

El maintainer aceptó las 7 recomendaciones del explore sin cambios:

1. **Sentido físico de avance**: se fija la convención de la referencia C++ (`IN2` = PWM de
   avance, `IN1` bajo; simétrico para atrás) con constantes `INVERT_LEFT` / `INVERT_RIGHT` por
   lado como escape hatch. La validación física se hace con batería durante la fase on-device;
   si el sentido resulta invertido, se corrige con la constante (sin tocar la convención).
2. **Signo de `omega`**: `+omega = giro a la izquierda` (CCW), igual que `turnLeft` de la
   referencia. La futura web debe respetarlo.
3. **Dead-man timeout**: 1000 ms sin `cmd:move` → parada (coast) desde la motor task.
4. **Deadband**: `ε = 0.05` sobre `v` y `omega` antes del mixing.
5. **Límite de velocidad en bring-up**: `MAX_SPEED = 0.7` para la primera prueba con batería;
   se sube a 1.0 tras confirmar control (mismo cambio o corrección acotada).
6. **`cmd:stop` = coast** (ambos IN en bajo). El freno (ambos IN en alto) queda diferido.
7. **GPIO12 como PWM de motor**: aceptado (la referencia ya maneja este kit así). Caveat
   documentado: un cambio de cableado podría afectar el strapping al bootear (MTDI).

## Restricción hardware (regla dura para todos los artefactos)

Sin protección contra USB + batería simultáneos en este rig. Pruebas de motores on-device:
**flashear por USB → desconectar USB → conectar batería**. Con batería no hay consola serial:
la verificación es por observación física del maintainer + estado del hub (`/robots`).
Si el maintainer resuelve la limitante electrónica, el flujo se actualiza (skill del rig +
artefactos).

## Cuestiones técnicas (resueltas por explore/proposal/design, no requieren decisión humana)

1. **Modelo de comando**: envelope v0 `cmd:move {v, omega}` (diferencial) y `cmd:stop`; el
   firmware solo recibe (keep-out de envío del cambio anterior se mantiene; telemetría = fase 3).
2. **Mixing**: `left = clamp(v - omega)`, `right = clamp(v + omega)` con normalización si el
   rango excede ±1 y deadband `ε` aplicada antes.
3. **PWM**: LEDC timer0 compartido, canales 0–3, 1 kHz, 8-bit; duty derivado de
   `get_max_duty()` (devuelve 256, no 255); `LedcTimerDriver` compartido por `&timer` (no `Rc`,
   que no es `Send`); `Gpio12..15` ya implementan `OutputPin` (sin `unsafe`).
4. **Parseo**: `serde`/`serde_json` (ya declarados en `Cargo.toml`, sin uso aún) dentro de la
   motor task; el callback WS solo reenvía `Text(String)` y pérdidas de conexión por canal
   (segundo canal `MotorSignal`), sin bloquear el callback.
5. **Paradas**: 6 disparadores centralizados en la motor task — arranque (motores detenidos),
   desconexión WS, error de parseo, comando inválido, dead-man 1 s (`recv_timeout`), `cmd:stop`.
6. **Config**: límites/velocidad por ahora constantes del módulo; NVS extendido queda como
   opción si el maintainer quiere cambiarlos sin recompilar (no bloqueante).
