//! Parseo y validación del envelope v0 (`cmd:move` / `cmd:stop`) y mezcla
//! diferencial pura. Sin hardware, sin I/O; no importa `crate::motors`.

use serde::Deserialize;
use std::fmt;

/// Umbral de banda muerta aplicado a las entradas antes de mezclar.
pub const DEADBAND: f32 = 0.05;

/// Velocidades por lado resultantes de la mezcla diferencial.
#[derive(Debug)]
pub struct SideSpeeds {
    pub left: f32,
    pub right: f32,
}

/// Comando ya validado y listo para el actuado de motores.
#[derive(Debug)]
pub enum Cmd {
    Move { v: f32, omega: f32 },
    Stop,
}

/// Errores de parseo/validación; todos terminan en parada (coast) en la tarea de motores.
#[derive(Debug)]
pub enum CommandError {
    MalformedJson,
    WrongShape,
    UnknownType,
    InvalidValue,
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    r#type: String,
    payload: Option<serde_json::Value>,
}

struct MovePayload {
    v: f32,
    omega: f32,
}

// Deserialización manual en vez de `#[derive(Deserialize)]`: leer `v`/`omega`
// como `f64` evita la ruta `copysign` de `serde_core::f32::deserialize` (f32
// desde f64), cuyo constant pool `[-1.0, 1.0]` dispara un bug de selección
// `PCREL_WRAPPER` en el backend Xtensa (LLVM 21). El `as f32` preserva el
// comportamiento: `parse` rechaza valores no finitos o fuera de rango.
impl<'de> Deserialize<'de> for MovePayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct MovePayloadVisitor;

        impl<'de> serde::de::Visitor<'de> for MovePayloadVisitor {
            type Value = MovePayload;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a cmd:move payload with v and omega")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut v: Option<f64> = None;
                let mut omega: Option<f64> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "v" => v = Some(map.next_value::<f64>()?),
                        "omega" => omega = Some(map.next_value::<f64>()?),
                        _ => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                let v = v.ok_or_else(|| serde::de::Error::missing_field("v"))?;
                let omega = omega.ok_or_else(|| serde::de::Error::missing_field("omega"))?;
                Ok(MovePayload {
                    v: v as f32,
                    omega: omega as f32,
                })
            }
        }

        deserializer.deserialize_struct("MovePayload", &["v", "omega"], MovePayloadVisitor)
    }
}

/// Parseo y validación del envelope v0 (§7.2). Nunca entra en pánico con entrada no confiable.
pub fn parse(s: &str) -> Result<Cmd, CommandError> {
    let value: serde_json::Value = serde_json::from_str(s).map_err(|_| CommandError::MalformedJson)?;
    let envelope: Envelope = serde_json::from_value(value).map_err(|_| CommandError::WrongShape)?;
    match envelope.r#type.as_str() {
        "cmd:move" => {
            let payload = envelope.payload.ok_or(CommandError::WrongShape)?;
            let move_payload: MovePayload =
                serde_json::from_value(payload).map_err(|_| CommandError::WrongShape)?;
            if !move_payload.v.is_finite()
                || !move_payload.omega.is_finite()
                || move_payload.v.abs() > 1.0
                || move_payload.omega.abs() > 1.0
            {
                return Err(CommandError::InvalidValue);
            }
            Ok(Cmd::Move {
                v: move_payload.v,
                omega: move_payload.omega,
            })
        }
        "cmd:stop" => Ok(Cmd::Stop),
        _ => Err(CommandError::UnknownType),
    }
}

/// Mezcla diferencial con banda muerta y normalización (§5, pasos 1–4).
pub fn mix(v: f32, omega: f32) -> SideSpeeds {
    let mut v = v;
    let mut omega = omega;
    if v.abs() < DEADBAND && omega.abs() < DEADBAND {
        return SideSpeeds {
            left: 0.0,
            right: 0.0,
        };
    }
    if v.abs() < DEADBAND {
        v = 0.0;
    }
    if omega.abs() < DEADBAND {
        omega = 0.0;
    }
    let mut left = v - omega;
    let mut right = v + omega;
    let m = left.abs().max(right.abs());
    if m > 1.0 {
        left /= m;
        right /= m;
    }
    SideSpeeds {
        left: left.clamp(-1.0, 1.0),
        right: right.clamp(-1.0, 1.0),
    }
}
