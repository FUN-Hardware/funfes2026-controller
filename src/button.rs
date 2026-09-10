use embassy_time::{Duration, Instant};
use esp_hal::gpio::Input;

pub struct Button<'a> {
    pin: Input<'a>,
    last_press: Instant,
}

impl<'a> Button<'a> {
    pub fn new(pin: Input<'a>) -> Self {
        Self {
            pin,
            last_press: Instant::now(),
        }
    }

    pub async fn wait_for_press(&mut self) {
        loop {
            self.pin.wait_for_falling_edge().await;
            let now = Instant::now();
            let duration = now - self.last_press;
            self.last_press = now;
            if duration > Duration::from_millis(50) {
                break;
            }
        }
    }

    pub async fn wait_for_release(&mut self) {
        loop {
            self.pin.wait_for_rising_edge().await;
            let now = Instant::now();
            let duration = now - self.last_press;
            self.last_press = now;
            if duration > Duration::from_millis(50) {
                break;
            }
        }
    }

    pub async fn wait_for_press_duration(&mut self) -> Duration {
        self.wait_for_press().await;
        let start = Instant::now();
        self.wait_for_release().await;
        Instant::now() - start
    }
}
