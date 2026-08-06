use bmi2::{
    Bmi2, I2cAddr,
    config::BMI270_CONFIG_FILE,
    interface::I2cInterface,
    types::{Burst, GyrRange, GyrRangeVal, OisRange, PwrCtrl},
};
use embassy_executor;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch};
use embassy_time::{Duration, Ticker};
use esp_hal::{Blocking, delay::Delay, i2c::master::I2c};

const RANGE: GyrRangeVal = GyrRangeVal::Range2000;
const RANGE_NUM: f32 = 2000.0;
const ALPHA: f32 = 0.1;
const SAMPLE_RATE: f32 = 0.01;

pub struct Gyro<'a, const N: usize> {
    pitch_speed: f32,
    yaw_speed: f32,
    pitch_angle: f32,
    yaw_angle: f32,
    imu: Bmi2<I2cInterface<I2c<'a, Blocking>>, Delay, N>,
    gyro_watch: watch::Sender<'a, CriticalSectionRawMutex, (f32, f32), 2>,
}

impl<'a, const N: usize> Gyro<'a, N> {
    pub fn new(
        i2c: I2c<'a, Blocking>,
        gyro_watch: watch::Sender<'a, CriticalSectionRawMutex, (f32, f32), 2>,
    ) -> Self {
        let mut imu = Bmi2::new_i2c(i2c, Delay::new(), I2cAddr::default(), Burst::default());

        imu.init(&BMI270_CONFIG_FILE)
            .expect("failed to init bmi270");
        imu.set_pwr_ctrl(PwrCtrl {
            gyr_en: true,
            acc_en: false,
            aux_en: false,
            temp_en: false,
        })
        .expect("failed to set pwr ctrl");

        imu.set_gyr_range(GyrRange {
            ois_range: OisRange::Range2000,
            range: RANGE,
        })
        .expect("failed to set gyr range");

        Self {
            pitch_speed: 0.0,
            yaw_speed: 0.0,
            pitch_angle: 0.0,
            yaw_angle: 0.0,
            imu,
            gyro_watch,
        }
    }

    fn sensor_read(&mut self) {
        let data = self.imu.get_gyr_data().unwrap();
        self.pitch_speed = Self::calc_ave(self.pitch_speed, Self::raw_to_degrees(data.y));
        self.yaw_speed = Self::calc_ave(self.yaw_speed, Self::raw_to_degrees(data.z));

        self.pitch_angle += self.pitch_speed * SAMPLE_RATE;
        self.yaw_angle += self.yaw_speed * SAMPLE_RATE;
        self.gyro_watch.send((self.pitch_angle, self.yaw_angle));
    }

    fn raw_to_degrees(raw: i16) -> f32 {
        raw as f32 / 32768.0 * RANGE_NUM
    }

    fn calc_ave(prev_ave: f32, new_val: f32) -> f32 {
        prev_ave * ALPHA + new_val * (1.0 - ALPHA)
    }
}

#[embassy_executor::task]
pub async fn sensor_task(mut gyro: Gyro<'static, 512>) {
    let mut ticker = Ticker::every(Duration::from_millis((SAMPLE_RATE * 1000.0) as u64));

    loop {
        ticker.next().await;
        gyro.sensor_read();
    }
}
