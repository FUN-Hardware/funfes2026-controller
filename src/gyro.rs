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

use crate::types::{CalibKind, CalibStatus};

const RANGE: GyrRangeVal = GyrRangeVal::Range2000;
const RANGE_NUM: f32 = 2000.0;
const ALPHA: f32 = 0.1;
const SAMPLE_RATE: f32 = 0.01;
const STATIONARY_SAMPLE_COUNT: usize = 500;

pub struct Gyro<'a, const N: usize> {
    pitch_speed: f32,
    yaw_speed: f32,
    pitch_angle: f32,
    yaw_angle: f32,
    pitch_offset: f32,
    yaw_offset: f32,
    imu: Bmi2<I2cInterface<I2c<'a, Blocking>>, Delay, N>,
    gyro_watch: watch::Sender<'a, CriticalSectionRawMutex, (f32, f32), 3>,
    calib_receiver: watch::Receiver<'a, CriticalSectionRawMutex, CalibStatus, 3>,
    calib_sender: watch::Sender<'a, CriticalSectionRawMutex, CalibStatus, 3>,
    stationary_samples: [(f32, f32); STATIONARY_SAMPLE_COUNT],
    stationary_sample_count: usize,
}

impl<'a, const N: usize> Gyro<'a, N> {
    pub fn new(
        i2c: I2c<'a, Blocking>,
        gyro_watch: watch::Sender<'a, CriticalSectionRawMutex, (f32, f32), 3>,
        calib_receiver: watch::Receiver<'a, CriticalSectionRawMutex, CalibStatus, 3>,
        calib_sender: watch::Sender<'a, CriticalSectionRawMutex, CalibStatus, 3>,
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
            pitch_offset: 0.0,
            yaw_offset: 0.0,
            imu,
            calib_receiver,
            calib_sender,
            gyro_watch,
            stationary_samples: [(0.0, 0.0); STATIONARY_SAMPLE_COUNT],
            stationary_sample_count: 0,
        }
    }

    fn sensor_read(&mut self) {
        let data = self.imu.get_gyr_data().unwrap();
        let pitch_rate = Self::raw_to_degrees(data.y);
        let yaw_rate = Self::raw_to_degrees(data.z);

        if self.calib_receiver.try_get() == Some(CalibStatus::Running(CalibKind::Stationary)) {
            self.record_stationary_sample(pitch_rate, yaw_rate);
        }

        self.pitch_speed = Self::calc_ave(self.pitch_speed, pitch_rate - self.pitch_offset);
        self.yaw_speed = Self::calc_ave(self.yaw_speed, yaw_rate - self.yaw_offset);

        self.pitch_angle += self.pitch_speed * SAMPLE_RATE;
        self.yaw_angle += self.yaw_speed * SAMPLE_RATE;
        self.gyro_watch.send((self.pitch_angle, self.yaw_angle));
    }

    fn record_stationary_sample(&mut self, pitch_rate: f32, yaw_rate: f32) {
        self.stationary_samples[self.stationary_sample_count] = (pitch_rate, yaw_rate);
        self.stationary_sample_count += 1;

        if self.stationary_sample_count == STATIONARY_SAMPLE_COUNT {
            self.stationary_sample_count = 0;

            let (pitch_sum, yaw_sum) = self
                .stationary_samples
                .iter()
                .fold((0.0, 0.0), |(p, y), (sp, sy)| (p + sp, y + sy));

            self.pitch_offset = pitch_sum / STATIONARY_SAMPLE_COUNT as f32;
            self.yaw_offset = yaw_sum / STATIONARY_SAMPLE_COUNT as f32;

            self.calib_sender.send(CalibStatus::Idle);
        }
    }

    fn raw_to_degrees(raw: i16) -> f32 {
        raw as f32 / 32768.0 * RANGE_NUM
    }

    fn calc_ave(prev_ave: f32, new_val: f32) -> f32 {
        prev_ave * ALPHA + new_val * (1.0 - ALPHA)
    }
}

#[embassy_executor::task]
pub async fn gyro_task(mut gyro: Gyro<'static, 512>) {
    let mut ticker = Ticker::every(Duration::from_millis((SAMPLE_RATE * 1000.0) as u64));

    loop {
        ticker.next().await;
        gyro.sensor_read();
    }
}
