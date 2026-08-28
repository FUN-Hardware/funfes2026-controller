use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Receiver};
use embassy_time::{Duration, Ticker};

use esp_println::println;

#[embassy_executor::task]
pub async fn display_task() {
    todo!()
}

#[embassy_executor::task]
pub async fn json_output_task(
    mut orientation_range_receiver: Receiver<'static, CriticalSectionRawMutex, [(f32, f32); 2], 1>,
    mut gyro_watch: Receiver<'static, CriticalSectionRawMutex, (f32, f32), 3>,
) {
    orientation_range_receiver.get().await;
    let mut ticker = Ticker::every(Duration::from_millis(1000));
    loop {
        let orientation = gyro_watch.get().await;
        let range = orientation_range_receiver.try_get().unwrap();
        let x = clamp(orientation.0, range[0].0, range[0].1);
        let y = clamp(orientation.1, range[1].1, range[1].0);

        println!("\"x\": {x}, \"y\": {y}");

        ticker.next().await;
    }
}

fn clamp(value: f32, zero: f32, one: f32) -> f32 {
    let x = (value - zero) / (one - zero);
    f32::max(0.0, f32::min(1.0, x))
}

#[embassy_executor::task]
pub async fn sound_task() {
    todo!()
}
