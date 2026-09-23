use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Receiver};
use embassy_time::{Duration, Ticker};
use esp_println::println;

use crate::types::*;

#[cfg(debug_assertions)]
const D: Duration = Duration::from_millis(1000);

#[cfg(not(debug_assertions))]
const D: Duration = Duration::from_millis(20);

#[embassy_executor::task]
pub async fn display_task(
    mut ammo_receiver: Receiver<'static, CriticalSectionRawMutex, u8, 3>,
    mut calib_receiver: Receiver<'static, CriticalSectionRawMutex, CalibStatus, 3>,
) {
    ammo_receiver.get().await;
    calib_receiver.get().await;

    todo!()
}

#[embassy_executor::task]
pub async fn json_output_task(
    mut orientation_range_receiver: Receiver<'static, CriticalSectionRawMutex, [(f32, f32); 2], 1>,
    mut gyro_watch: Receiver<'static, CriticalSectionRawMutex, (f32, f32), 3>,
    mut ammo_receiver: Receiver<'static, CriticalSectionRawMutex, u8, 3>,
) {
    orientation_range_receiver.get().await;
    let mut ticker = Ticker::every(D);
    loop {
        let orientation = gyro_watch.get().await;
        let range = orientation_range_receiver.try_get().unwrap();
        let x = clamp(orientation.0, range[0].0, range[0].1);
        let y = clamp(orientation.1, range[1].1, range[1].0);

        let ammo = ammo_receiver.try_get().unwrap_or(0);

        println!("{{\"x\": {x}, \"y\": {y}, \"ammo\": {ammo}}}");

        ticker.next().await;
    }
}

fn clamp(value: f32, zero: f32, one: f32) -> f32 {
    let x = (value - zero) / (one - zero);
    x.clamp(0.0, 1.0)
}
