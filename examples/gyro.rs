// ジャイロセンサーからの情報取得てすと

#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use bmi2::{
    Bmi2, I2cAddr,
    config::BMI270_CONFIG_FILE,
    types::{Burst, GyrRange, GyrRangeVal, OisRange, PwrCtrl},
};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer, Ticker};
use esp_hal::{clock::CpuClock, delay::Delay, i2c::master::I2c, timer::timg::TimerGroup};
use esp_println::println;

#[panic_handler]
fn panic(p: &core::panic::PanicInfo) -> ! {
    println!("Panic: {}", p);
    loop {}
}

const RANGE: GyrRangeVal = GyrRangeVal::Range2000;
const RANGE_NUM: f32 = 2000.0;
const ALPHA: f32 = 0.1;
const SAMPLE_RATE: f32 = 0.01;

esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    println!("Boot");

    let i2c = I2c::new(peripherals.I2C0, Default::default())
        .unwrap()
        .with_sda(peripherals.GPIO47)
        .with_scl(peripherals.GPIO48);
    let mut imu = Bmi2::<_, _, 512>::new_i2c(i2c, Delay::new(), I2cAddr::Default, Burst::default());

    println!("I2C initialized");

    imu.init(&BMI270_CONFIG_FILE).expect("Failed to initialize");

    println!("Initialized");

    imu.set_pwr_ctrl(PwrCtrl {
        gyr_en: true,
        acc_en: false,
        aux_en: false,
        temp_en: false,
    })
    .expect("Failed to set power control");
    imu.set_gyr_range(GyrRange {
        ois_range: OisRange::Range2000,
        range: RANGE,
    })
    .expect("Failed to set gyro range");

    println!("Powered on");

    Timer::after(Duration::from_millis(50)).await;

    // TODO: Spawn some tasks
    let _ = spawner;

    let mut x_ave = 0.0;
    let mut y_ave = 0.0;
    let mut z_ave = 0.0;

    let mut x_angle = 0.0;
    let mut y_angle = 0.0;
    let mut z_angle = 0.0;
    
    let mut i = 0;

    let mut ticker = Ticker::every(Duration::from_millis((SAMPLE_RATE * 1000.0) as u64));

    loop {
        ticker.next().await;
        i += 1;
        let data = imu.get_gyr_data().unwrap();
        x_ave = calc_ave(x_ave, raw_to_degrees(data.x));
        y_ave = calc_ave(y_ave, raw_to_degrees(data.y));
        z_ave = calc_ave(z_ave, raw_to_degrees(data.z));
        x_angle += x_ave * SAMPLE_RATE;
        y_angle += y_ave * SAMPLE_RATE;
        z_angle += z_ave * SAMPLE_RATE;
        
        if i % 10 == 0 {
            println!(
                "x: {:15}, y: {:15}, z: {:15}",
                x_angle,
                y_angle,
                z_angle
            );
            i = 1;
        }
    }
}

fn raw_to_degrees(raw: i16) -> f32 {
    raw as f32 / 32768.0 * RANGE_NUM
}

fn calc_ave(prev_ave: f32, new_val: f32) -> f32 {
    prev_ave * ALPHA + new_val * (1.0 - ALPHA)
}