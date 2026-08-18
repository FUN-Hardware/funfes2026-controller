use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel, signal, watch};
use embassy_time::{Duration, Instant, Ticker};
use esp_hal::{Blocking, delay::Delay, gpio::Input, i2c::master::I2c};
use esp_println::println;

use crate::types::{CalibKind, CalibStatus, GameEvent};

pub struct TriggerButton<'a> {
    trigger_button: Input<'a>,
    trigger_sender: channel::Sender<'a, CriticalSectionRawMutex, (), 3>,
}

impl<'a> TriggerButton<'a> {
    pub fn new(
        trigger_button: Input<'a>,
        trigger_sender: channel::Sender<'a, CriticalSectionRawMutex, (), 3>,
    ) -> Self {
        Self {
            trigger_button,
            trigger_sender,
        }
    }
}

#[embassy_executor::task]
pub async fn trigger_task(mut trigger_button: TriggerButton<'static>) {
    let mut last_press = Instant::now();
    loop {
        trigger_button.trigger_button.wait_for_falling_edge().await;
        if last_press.elapsed() > Duration::from_millis(50) {
            trigger_button.trigger_sender.send(()).await;
            println!("triggered");
        }
        last_press = Instant::now();
    }
}

#[embassy_executor::task]
pub async fn reload_task() {
    todo!()
}

pub struct CalibButton<'a> {
    gyro_calib: &'a signal::Signal<CriticalSectionRawMutex, CalibKind>,
    game_event_sender: channel::Sender<'a, CriticalSectionRawMutex, GameEvent, 3>,
    trigger_receiver: channel::Receiver<'a, CriticalSectionRawMutex, (), 3>,
    calib_button: Input<'a>,
    calib_status: CalibStatus,
}

impl<'a> CalibButton<'a> {
    pub fn new(
        gyro_calib: &'a signal::Signal<CriticalSectionRawMutex, CalibKind>,
        game_event_sender: channel::Sender<'a, CriticalSectionRawMutex, GameEvent, 3>,
        trigger_receiver: channel::Receiver<'a, CriticalSectionRawMutex, (), 3>,
        calib_button: Input<'a>,
    ) -> Self {
        Self {
            gyro_calib,
            game_event_sender,
            trigger_receiver,
            calib_button,
            calib_status: CalibStatus::Idle,
        }
    }
}

#[embassy_executor::task]
pub async fn calib_button_task(mut button: CalibButton<'static>) {
    let mut last_press = None;
    loop {
        println!("Running");
        match select(
            button.calib_button.wait_for_any_edge(),
            button.trigger_receiver.receive(),
        )
        .await
        {
            Either::First(()) => {
                match last_press {
                    Some(instant) => {
                        let duration = Instant::now() - instant;
                        if duration < Duration::from_millis(50) {
                            // 無視
                        } else if duration < Duration::from_millis(1000) {
                            // 短押し
                            if button.calib_status == CalibStatus::Selecting {
                                button.calib_status = CalibStatus::Running(CalibKind::Orientation);
                            }
                        } else {
                            // 長押し
                            if button.calib_status == CalibStatus::Selecting {
                                button.calib_status = CalibStatus::Running(CalibKind::Stationary);
                            } else if button.calib_status == CalibStatus::Idle {
                                button.calib_status = CalibStatus::Selecting;
                            }
                        }
                        last_press = None;
                    }
                    None => {
                        last_press = Some(Instant::now());
                    }
                }
                println!("{:?}", button.calib_status);
            }
            Either::Second(()) => {}
        }
    }
}
