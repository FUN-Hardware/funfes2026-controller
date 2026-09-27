use embassy_embedded_hal::shared_bus::blocking::i2c::I2cDevice;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal, watch};
use esp_hal::{Blocking, i2c::master::I2c as EspI2c};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CalibKind {
    Orientation,
    Stationary,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CalibStatus {
    Idle,
    Selecting,
    Running(CalibKind),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SoundEvent {
    Fire,
    Reload,
}

pub type SharedI2c = I2cDevice<'static, CriticalSectionRawMutex, EspI2c<'static, Blocking>>;

/// 受信者は gyro_task / trigger_router_task / display_task / main の4つ。
const CALIB_RECEIVERS: usize = 4;

pub type CalibWatch = watch::Watch<CriticalSectionRawMutex, CalibStatus, CALIB_RECEIVERS>;
pub type CalibSender<'a> = watch::Sender<'a, CriticalSectionRawMutex, CalibStatus, CALIB_RECEIVERS>;
pub type CalibReceiver<'a> =
    watch::Receiver<'a, CriticalSectionRawMutex, CalibStatus, CALIB_RECEIVERS>;

/// 4隅キャリブレーションの結果 `[(pitch_min, pitch_max), (yaw_min, yaw_max)]`。
pub type OrientationRange = [(f32, f32); 2];

/// 受信者は json_output_task と Gyro の2つ。
pub type OrientationRangeWatch = watch::Watch<CriticalSectionRawMutex, OrientationRange, 2>;
pub type OrientationRangeSender<'a> =
    watch::Sender<'a, CriticalSectionRawMutex, OrientationRange, 2>;
pub type OrientationRangeReceiver<'a> =
    watch::Receiver<'a, CriticalSectionRawMutex, OrientationRange, 2>;

/// リロード成立時に「今向いている方向を出力の中心(0.5, 0.5)にし直せ」とGyroへ伝える。
/// 連打されても最新の1件だけ残ればよいのでSignalを使う。
pub type RecenterSignal = signal::Signal<CriticalSectionRawMutex, ()>;
