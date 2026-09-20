use embassy_embedded_hal::shared_bus::blocking::i2c::I2cDevice;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
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
