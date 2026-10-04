use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::Input;

/// この時間レベルが変わらず保たれたら、押下/解放として確定する
const STABLE_TIME: Duration = Duration::from_millis(10);

pub struct Button<'a> {
    pin: Input<'a>,
}

impl<'a> Button<'a> {
    pub fn new(pin: Input<'a>) -> Self {
        Self { pin }
    }

    /// Low が STABLE_TIME 続いたら押下と確定する。
    /// 途中で High に戻ったらノイズとみなして待ち直す。
    pub async fn wait_for_press(&mut self) {
        loop {
            self.pin.wait_for_low().await;
            if let Either::First(()) =
                select(Timer::after(STABLE_TIME), self.pin.wait_for_high()).await
            {
                break;
            }
        }
    }

    /// High が STABLE_TIME 続いたら解放と確定する。
    /// エッジではなくレベルを待つので、すでに High なら即座に確認に入る。
    pub async fn wait_for_release(&mut self) {
        loop {
            self.pin.wait_for_high().await;
            if let Either::First(()) =
                select(Timer::after(STABLE_TIME), self.pin.wait_for_low()).await
            {
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
