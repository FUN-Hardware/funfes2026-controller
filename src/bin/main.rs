#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal, watch::Watch, channel::{Channel, self}};
use embassy_time::{Duration, Timer};
use esp_hal::{clock::CpuClock, i2c::master::I2c, timer::timg::TimerGroup, gpio::{Input, InputConfig, Pull}};
use esp_println::println;

use funfes2026_controller::{game, gyro, input, types::*};

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

    static GYRO_WATCH: Watch<CriticalSectionRawMutex, (f32, f32), 3> = Watch::new();
    static GYRO_CALIB: Watch<CriticalSectionRawMutex, CalibStatus, 3> = Watch::new();
    static TRIGGER_CHANNEL: Channel<CriticalSectionRawMutex, (), 3> = Channel::new();
    static GAME_EVENT_CHANNEL: Channel<CriticalSectionRawMutex, GameEvent, 3> = Channel::new();

    let gyro = gyro::Gyro::new(
        I2c::new(peripherals.I2C0, Default::default())
            .unwrap()
            .with_sda(peripherals.GPIO47)
            .with_scl(peripherals.GPIO48),
        GYRO_WATCH.sender(),
        GYRO_CALIB.receiver().unwrap(),
        GYRO_CALIB.sender(),
    );

    spawner.spawn(gyro::gyro_task(gyro)).unwrap();

    let trigger_button_config = InputConfig::default().with_pull(Pull::Up);
    let trigger_button = input::TriggerButton::new(Input::new(peripherals.GPIO12, trigger_button_config), TRIGGER_CHANNEL.sender());

    spawner.spawn(input::trigger_task(trigger_button)).unwrap();

    let calib_button_config = InputConfig::default().with_pull(Pull::Up);
    let calib_button = input::CalibButton::new(GYRO_CALIB.sender(), GAME_EVENT_CHANNEL.sender(), Input::new(peripherals.GPIO11, calib_button_config));

    spawner.spawn(input::calib_button_task(calib_button)).unwrap();

    let trigger_router = game::TriggerRouter::new(
        TRIGGER_CHANNEL.receiver(),
        GYRO_CALIB.receiver().unwrap(),
        GYRO_CALIB.sender(),
        GYRO_WATCH.receiver().unwrap(),
        GAME_EVENT_CHANNEL.sender(),
    );

    spawner.spawn(game::trigger_router_task(trigger_router)).unwrap();

    GYRO_CALIB.sender().send(CalibStatus::Running(CalibKind::Stationary));
    let mut receiver = GYRO_CALIB.receiver().unwrap();
    receiver.changed_and(|x| *x == CalibStatus::Running(CalibKind::Stationary)).await;

    let mut gyro_watch = GYRO_WATCH.receiver().unwrap();

    loop {
        Timer::after(Duration::from_secs(1)).await;
        let gyro_watch_value = gyro_watch.get().await;
        let calib_status = receiver.get().await;
        println!("{:?} {:?}", gyro_watch_value, calib_status);
    }
}
