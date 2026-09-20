#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use core::cell::RefCell;

use embassy_embedded_hal::shared_bus::blocking::i2c::I2cDevice;
use embassy_executor::Spawner;
use embassy_sync::{
    blocking_mutex::{Mutex, raw::CriticalSectionRawMutex},
    channel::Channel,
    watch::Watch,
};
use embassy_time::{Duration, Timer};
use esp_hal::{
    Blocking,
    clock::CpuClock,
    delay::Delay,
    gpio::{Input, InputConfig, Pull},
    i2c::master::I2c,
    timer::timg::TimerGroup,
};
use esp_println::println;

use funfes2026_controller::{audio, game, gyro, input, output, types::*};
use static_cell::StaticCell;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("Panic: {}", info);
    loop {}
}

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
    static AMMO_WATCH: Watch<CriticalSectionRawMutex, u8, 3> = Watch::new();
    static SOUND_EVENT_CHANNEL: Channel<CriticalSectionRawMutex, SoundEvent, 3> = Channel::new();
    static ORIENTATION_RANGE_WATCH: Watch<CriticalSectionRawMutex, [(f32, f32); 2], 1> =
        Watch::new();
    static I2C_BUS: StaticCell<Mutex<CriticalSectionRawMutex, RefCell<I2c<'static, Blocking>>>> =
        StaticCell::new();

    let i2c_bus = I2C_BUS.init(Mutex::new(RefCell::new(
        I2c::new(peripherals.I2C0, Default::default())
            .unwrap()
            .with_sda(peripherals.GPIO47)
            .with_scl(peripherals.GPIO48),
    )));

    let gyro = gyro::Gyro::new(
        I2cDevice::new(i2c_bus),
        GYRO_WATCH.sender(),
        GYRO_CALIB.receiver().unwrap(),
        GYRO_CALIB.sender(),
    );

    spawner.spawn(gyro::gyro_task(gyro)).unwrap();

    let trigger_button_config = InputConfig::default().with_pull(Pull::Up);
    let trigger_button = input::TriggerButton::new(
        Input::new(peripherals.GPIO10, trigger_button_config), //12はStickの横のボタン、仮置きしているだけ
        TRIGGER_CHANNEL.sender(),
    );

    spawner.spawn(input::trigger_task(trigger_button)).unwrap();

    let calib_button_config = InputConfig::default().with_pull(Pull::Up);
    let calib_button = input::CalibButton::new(
        GYRO_CALIB.sender(),
        Input::new(peripherals.GPIO11, calib_button_config),
    );

    spawner
        .spawn(input::calib_button_task(calib_button))
        .unwrap();

    let trigger_router = game::TriggerRouter::new(
        TRIGGER_CHANNEL.receiver(),
        GYRO_CALIB.receiver().unwrap(),
        GYRO_CALIB.sender(),
        GYRO_WATCH.receiver().unwrap(),
        AMMO_WATCH.sender(),
        SOUND_EVENT_CHANNEL.sender(),
        ORIENTATION_RANGE_WATCH.sender(),
    );

    spawner
        .spawn(game::trigger_router_task(trigger_router))
        .unwrap();

    let ammo_button_config = InputConfig::default().with_pull(Pull::Up);

    spawner
        .spawn(input::reload_task(
            AMMO_WATCH.sender(),
            SOUND_EVENT_CHANNEL.sender(),
            Input::new(peripherals.GPIO9, ammo_button_config), // 基板作成待ちのため、暫定的にM5StickS3内蔵ボタンを使用
        ))
        .unwrap();

    GYRO_CALIB
        .sender()
        .send(CalibStatus::Running(CalibKind::Stationary));
    let mut receiver = GYRO_CALIB.receiver().unwrap();
    receiver.changed_and(|x| *x == CalibStatus::Idle).await;
    GYRO_CALIB
        .sender()
        .send(CalibStatus::Running(CalibKind::Orientation));

    spawner
        .spawn(output::json_output_task(
            ORIENTATION_RANGE_WATCH.receiver().unwrap(),
            GYRO_WATCH.receiver().unwrap(),
            AMMO_WATCH.receiver().unwrap(),
        ))
        .unwrap();

    let mut delay = Delay::new();
    let mut i2c = I2cDevice::new(i2c_bus);

    let (tx, buf) = audio::init_i2s(
        peripherals.I2S0,
        peripherals.DMA_CH0,
        peripherals.GPIO18,
        peripherals.GPIO17,
        peripherals.GPIO15,
        peripherals.GPIO14,
    );
    audio::codec::init(&mut i2c, &mut delay, 60).unwrap();
    spawner
        .spawn(audio::sound_task(tx, buf, SOUND_EVENT_CHANNEL.receiver()))
        .unwrap();

    loop {
        Timer::after(Duration::from_secs(1)).await;
        let calib_status = receiver.get().await;
        funfes2026_controller::debug_println!("{:?}", calib_status);
    }
}
