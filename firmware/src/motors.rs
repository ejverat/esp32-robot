//! Control de motores: LEDC PWM (timer0, 1 kHz, 8-bit) sobre el L298N y la
//! tarea de motores que posee todas las decisiones de parada (seis disparadores
//! → un único `stop()` = coast). Bucle bloqueante; sin `embassy-executor`.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use esp_idf_svc::hal::gpio::{Gpio12, Gpio13, Gpio14, Gpio15};
use esp_idf_svc::hal::ledc::config::TimerConfig;
use esp_idf_svc::hal::ledc::{LedcDriver, LedcTimerDriver, LowSpeed, LEDC};
use esp_idf_svc::sys::EspError;

use crate::command::{self, Cmd};

/// Señales que el callback WebSocket reenvía a la tarea de motores.
pub enum MotorSignal {
    Text(String),
    ConnectionLost,
}

/// Canal no acotado (envío no bloqueante) hacia la tarea de motores.
pub type MotorSender = Sender<MotorSignal>;

/// Tope de velocidad de arranque (subir a 1.0 tras confirmar control en batería).
pub const MAX_SPEED: f32 = 0.7;
/// Corrección de sentido por lado; solo se niega el signo, nunca la convención.
/// LEFT invertido: validado en banco con batería (L1/L2 daban el sentido opuesto al
/// esperado con el cableado IN1/IN2 de este kit); RIGHT correcto sin inversión.
pub const INVERT_LEFT: bool = true;
pub const INVERT_RIGHT: bool = false;
/// Ventana de "hombre muerto": sin `cmd:move` válido en 1 s → coast.
pub const DEADMAN: Duration = Duration::from_millis(1000);

/// Estado PWM de los cuatro canales LEDC; `init` es el único constructor.
pub struct Motors {
    in1_left: LedcDriver<'static>,
    in2_left: LedcDriver<'static>,
    in1_right: LedcDriver<'static>,
    in2_right: LedcDriver<'static>,
    _timer: LedcTimerDriver<'static, LowSpeed>,
}

pub fn init(
    ledc: LEDC,
    gpio12: Gpio12<'static>,
    gpio13: Gpio13<'static>,
    gpio14: Gpio14<'static>,
    gpio15: Gpio15<'static>,
) -> Result<Motors, EspError> {
    // Timer0 en 1 kHz / 8-bit (config por defecto), compartido vía `&timer`.
    let timer = LedcTimerDriver::new(ledc.timer0, &TimerConfig::new())?;
    let in1_left = LedcDriver::new(ledc.channel0, &timer, gpio12)?; // LEFT IN1 = ch0 → GPIO12
    let in2_left = LedcDriver::new(ledc.channel1, &timer, gpio13)?; // LEFT IN2 = ch1 → GPIO13
    let in1_right = LedcDriver::new(ledc.channel2, &timer, gpio15)?; // RIGHT IN1 = ch2 → GPIO15
    let in2_right = LedcDriver::new(ledc.channel3, &timer, gpio14)?; // RIGHT IN2 = ch3 → GPIO14
    Ok(Motors {
        in1_left,
        in2_left,
        in1_right,
        in2_right,
        _timer: timer,
    })
}

impl Motors {
    /// Aplica `{v, omega}`: mezcla → clamp MAX_SPEED → invert → duty → encode.
    /// Devuelve `true` si algún lado tiene duty distinto de cero (estado `moving`).
    pub fn apply(&mut self, v: f32, omega: f32) -> bool {
        let mixed = command::mix(v, omega);
        let mut left = mixed.left.clamp(-MAX_SPEED, MAX_SPEED);
        let mut right = mixed.right.clamp(-MAX_SPEED, MAX_SPEED);
        if INVERT_LEFT {
            left = -left;
        }
        if INVERT_RIGHT {
            right = -right;
        }
        Self::drive_side(&mut self.in1_left, &mut self.in2_left, left);
        Self::drive_side(&mut self.in1_right, &mut self.in2_right, right);
        left != 0.0 || right != 0.0
    }

    /// Parada (coast): duty 0 en los cuatro canales. Idempotente.
    pub fn stop(&mut self) {
        Self::set_duty_warn(&mut self.in1_left, 0);
        Self::set_duty_warn(&mut self.in2_left, 0);
        Self::set_duty_warn(&mut self.in1_right, 0);
        Self::set_duty_warn(&mut self.in2_right, 0);
    }

    fn drive_side(in1: &mut LedcDriver<'static>, in2: &mut LedcDriver<'static>, speed: f32) {
        let max_duty = in1.get_max_duty();
        let duty = ((speed.abs() * max_duty as f32).round() as u32).min(max_duty);
        let (duty_in1, duty_in2) = if speed > 0.0 {
            (0, duty) // forward: IN2 PWM, IN1 low
        } else if speed < 0.0 {
            (duty, 0) // reverse: IN1 PWM, IN2 low
        } else {
            (0, 0) // coast
        };
        Self::set_duty_warn(in1, duty_in1);
        Self::set_duty_warn(in2, duty_in2);
    }

    fn set_duty_warn(driver: &mut LedcDriver<'static>, duty: u32) {
        if let Err(e) = driver.set_duty(duty) {
            log::warn!("set_duty failed: {e}");
        }
    }
}

/// Tarea de motores: dueña de todo el estado PWM y de toda decisión de parada.
pub fn run(rx: Receiver<MotorSignal>, mut motors: Motors) {
    let mut moving = false;
    loop {
        match rx.recv_timeout(DEADMAN) {
            Ok(MotorSignal::Text(s)) => match command::parse(&s) {
                Ok(Cmd::Move { v, omega }) => {
                    moving = motors.apply(v, omega);
                }
                Ok(Cmd::Stop) => {
                    motors.stop();
                    moving = false;
                }
                Err(e) => {
                    log::warn!("invalid command {e:?}; coasting");
                    motors.stop();
                    moving = false;
                }
            },
            Ok(MotorSignal::ConnectionLost) => {
                motors.stop();
                moving = false;
            }
            Err(RecvTimeoutError::Timeout) => {
                if moving {
                    log::warn!("dead-man: no cmd:move for 1s; coasting");
                    motors.stop();
                    moving = false;
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                motors.stop();
                log::warn!("motor senders dropped; parking");
                thread::park();
            }
        }
    }
}
