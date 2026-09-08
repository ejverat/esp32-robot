# Servidor — Hub WebSocket (Rust / Axum)

Hub central del proyecto: hace de *relay* entre el/los robot(es) y los clientes
(navegador, agente IA). No interpreta el `payload`; solo enruta por `robot_id` y por
la dirección de la conexión.

## Endpoints

| Método | Ruta | Descripción |
|--------|------|-------------|
| GET | `/health` | Liveness: responde `ok`. |
| GET | `/robots` | Lista JSON de robots conectados. |
| GET | `/ws/robot?robot_id=<id>` | WebSocket para el robot con id `<id>`. |
| GET | `/ws/client` | WebSocket para clientes (navegador / IA). |

## Mensajes (envelope)

```json
{
  "type": "cmd:move",
  "id": "c1f2-...",
  "ts": 1725123456,
  "robot_id": "a1",
  "payload": { "v": 0.5, "omega": 0.2 }
}
```

Tipos (campo `type`): `cmd:move`, `cmd:stop`, `cmd:camera`, `telemetry`, `video`,
`config`, `ping`, `pong`, `register`, `robots:list`, `error`.

## Enrutado

- **robot → servidor → clientes:** todo lo que envía un robot (telemetría, video
  binario) se difunde a todos los clientes conectados.
- **cliente → servidor → robot:** el mensaje debe llevar `robot_id`; se enruta solo a
  ese robot. Sin `robot_id`, se difunde a **todos** los robots (p. ej. "detener flota").
- Al conectar, el cliente recibe un `robots:list` con los robots activos.

## Correr

```sh
nix develop .#server
cd server
cargo run            # escucha en 0.0.0.0:8080 (override: HUB_ADDR=127.0.0.1:8080)
```

## Probar a mano

Con el server corriendo, abrí dos terminales (`websocat` o similar):

```sh
# terminal 1 — robot simulado
websocat "ws://127.0.0.1:8080/ws/robot?robot_id=a1"

# terminal 2 — cliente
websocat "ws://127.0.0.1:8080/ws/client"
```

En el cliente enviá:

```json
{"type":"cmd:move","robot_id":"a1","payload":{"v":0.5,"omega":0.2}}
```

El robot lo recibe. Si el robot envía:

```json
{"type":"telemetry","robot_id":"a1","payload":{"battery":3.9}}
```

el cliente lo recibe.

## Tests

```sh
cargo test          # unit (hub) + e2e (websocket robot↔cliente)
```
