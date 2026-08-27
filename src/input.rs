use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel, watch};
use embassy_time::{Duration, Instant};
use esp_hal::gpio::Input;
use esp_println::println;

use crate::types::{CalibKind, CalibStatus, GameEvent};

pub struct TriggerButton<'a> {
    trigger_button: Input<'a>,
    trigger_sender: channel::Sender<'a, CriticalSectionRawMutex, (), 3>,
    last_press: Instant,
}

impl<'a> TriggerButton<'a> {
    pub fn new(
        trigger_button: Input<'a>,
        trigger_sender: channel::Sender<'a, CriticalSectionRawMutex, (), 3>,
    ) -> Self {
        Self {
            trigger_button,
            trigger_sender,
            last_press: Instant::now(),
        }
    }

    fn debounced(&mut self) -> bool {
        let ok = self.last_press.elapsed() > Duration::from_millis(50);
        self.last_press = Instant::now();
        ok
    }

    async fn fire(&self) {
        self.trigger_sender.send(()).await;
        println!("triggered");
    }
}

#[embassy_executor::task]
pub async fn trigger_task(mut trigger_button: TriggerButton<'static>) {
    loop {
        trigger_button.trigger_button.wait_for_falling_edge().await;
        if trigger_button.debounced() {
            trigger_button.fire().await;
        }
    }
}

#[embassy_executor::task]
pub async fn reload_task() {
    todo!()
}

pub struct CalibButton<'a> {
    gyro_calib: watch::Sender<'a, CriticalSectionRawMutex, CalibStatus, 3>,
    game_event_sender: channel::Sender<'a, CriticalSectionRawMutex, GameEvent, 3>,
    calib_button: Input<'a>,
    last_press: Option<Instant>,
}

impl<'a> CalibButton<'a> {
    pub fn new(
        gyro_calib: watch::Sender<'a, CriticalSectionRawMutex, CalibStatus, 3>,
        game_event_sender: channel::Sender<'a, CriticalSectionRawMutex, GameEvent, 3>,
        calib_button: Input<'a>,
    ) -> Self {
        gyro_calib.send(CalibStatus::Idle);
        Self {
            gyro_calib,
            game_event_sender,
            calib_button,
            last_press: None,
        }
    }

    fn handle_edge(&mut self) {
        match self.last_press {
            Some(instant) => {
                self.classify_press(Instant::now() - instant);
                self.last_press = None;
            }
            None => {
                self.last_press = Some(Instant::now());
            }
        }
    }

    fn classify_press(&mut self, duration: Duration) {
        if duration < Duration::from_millis(50) {
            // 無視
        } else {
            let now_state = self
                .gyro_calib
                .try_get()
                .expect("failed to get gyro calib state");
            if duration < Duration::from_millis(1000) {
                // 短押し
                if now_state == CalibStatus::Selecting {
                    self.gyro_calib
                        .send(CalibStatus::Running(CalibKind::Orientation));
                }
            } else {
                // 長押し
                if now_state == CalibStatus::Selecting {
                    self.gyro_calib
                        .send(CalibStatus::Running(CalibKind::Stationary));
                } else if now_state == CalibStatus::Idle {
                    self.gyro_calib.send(CalibStatus::Selecting);
                }
            }
        }
    }
}

#[embassy_executor::task]
pub async fn calib_button_task(mut button: CalibButton<'static>) {
    loop {
        button.calib_button.wait_for_any_edge().await;
        button.handle_edge();
    }
}
