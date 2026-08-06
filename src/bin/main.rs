#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal, watch::Watch};
use embassy_time::{Duration, Timer};
use esp_hal::{clock::CpuClock, i2c::master::I2c, timer::timg::TimerGroup};
use esp_println::println;

use funfes2026_controller::sensor;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("Panic: {}", info);
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.2.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    static GYRO_WATCH: Watch<CriticalSectionRawMutex, (f32, f32), 2> = Watch::new();
    static GYRO_RESET: Signal<CriticalSectionRawMutex, ()> = Signal::new();

    let gyro = sensor::Gyro::new(
        I2c::new(peripherals.I2C0, Default::default())
            .unwrap()
            .with_sda(peripherals.GPIO47)
            .with_scl(peripherals.GPIO48),
        GYRO_WATCH.sender(),
    );

    println!("OK");

    spawner.spawn(sensor::sensor_task(gyro)).unwrap();

    let mut receiver = GYRO_WATCH.receiver().unwrap();

    loop {
        Timer::after(Duration::from_secs(1)).await;
        let gyro_data = receiver.get().await;
        println!("gyro_data: {:?}", gyro_data);
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0/examples
}
