use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Receiver};
use embassy_time::{Duration, Ticker};
use esp_println::println;

use crate::{display::Screen, types::*};

#[cfg(debug_assertions)]
const D: Duration = Duration::from_millis(1000);

#[cfg(not(debug_assertions))]
const D: Duration = Duration::from_millis(20);

#[embassy_executor::task]
pub async fn display_task(
    mut screen: Screen,
    mut ammo_receiver: Receiver<'static, CriticalSectionRawMutex, u8, 3>,
    mut calib_receiver: CalibReceiver<'static>,
) {
    let mut ammo = ammo_receiver.get().await;
    let mut calib = calib_receiver.get().await;

    loop {
        if let Err(e) = screen.render(calib, ammo) {
            crate::debug_println!("display error: {:?}", e);
        }

        // 負けた側の future を捨てても取りこぼしはない。
        // Receiver は changed() が完了したときだけ既読位置を進めるため、
        // 未読の変化は次のループで即座に拾える。
        match select(calib_receiver.changed(), ammo_receiver.changed()).await {
            Either::First(next) => calib = next,
            Either::Second(next) => ammo = next,
        }
    }
}

#[embassy_executor::task]
pub async fn json_output_task(
    mut orientation_range_receiver: OrientationRangeReceiver<'static>,
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
