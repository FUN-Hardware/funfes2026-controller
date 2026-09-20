//! M5StickS3 の音声ハード初期化（I2C側）。esp-hal 非依存（embedded-hal 1.0 のみ）。
//!
//! 1. M5PM1 (0x6E) の PYG2 = L3B 電源, PYG3 = スピーカーアンプ(AW8737) を有効化
//! 2. ES8311 (0x18) を初期化して音量を設定
//!
//! 呼び出しは spawn 前に blocking で1回だけ。終わったら I2c は BMI270 側へ渡してよい。

use embedded_hal::{delay::DelayNs, i2c::I2c};
use es8311::{ClockConfig, Es8311, Resolution};

/// サンプルレート（wav側 `-ar 16000` と一致させる）
pub const SAMPLE_RATE: u32 = 16_000;
/// MCLK = 256 × fs（es8311 crate の係数表に 4_096_000/16_000 が存在）
pub const MCLK_HZ: u32 = SAMPLE_RATE * 256;

const PM1_ADDR: u8 = 0x6E;
const ES8311_ADDR: u8 = 0x18;

// M5PM1 レジスタ（公式ヘッダ/データシート）
const PM1_REG_GPIO_MODE: u8 = 0x10; // bit n: 1=output
const PM1_REG_GPIO_OUT: u8 = 0x11; // bit n: 1=high
const PM1_REG_GPIO_IN: u8 = 0x12;
const PM1_REG_GPIO_DRV: u8 = 0x13; // bit n: 1=open-drain, 0=push-pull
const PM1_REG_GPIO_PUPD0: u8 = 0x14; // GPIO0-3, 2bit/pin: 00=none
const PM1_REG_GPIO_FUNC0: u8 = 0x16; // GPIO0-3, 2bit/pin: 00=標準GPIO

/// PM1 の GPIO2 = L3B（LCDバックライト/MIC/SPK 電源）
const PM1_GPIO_L3B: u8 = 2;
/// PM1 の GPIO3 = スピーカーアンプ有効
const PM1_GPIO_SPK: u8 = 3;

#[derive(Debug)]
pub enum AudioError<E> {
    Pm1(E),
    Codec(es8311::Error<E>),
}

/// レジスタの一部ビットだけ書き換える（read-modify-write）
fn pm1_modify<I: I2c>(i2c: &mut I, reg: u8, mask: u8, value: u8) -> Result<(), I::Error> {
    let mut b = [0u8];
    i2c.write_read(PM1_ADDR, &[reg], &mut b)?;
    b[0] = (b[0] & !mask) | (value & mask);
    i2c.write(PM1_ADDR, &[reg, b[0]])
}

/// PM1 の GPIO n を「標準GPIO・プルなし・push-pull・出力」にして level を出す
fn pm1_gpio_output<I: I2c>(i2c: &mut I, n: u8, high: bool) -> Result<(), I::Error> {
    debug_assert!(n <= 3);
    let two = 0b11u8 << (n * 2);
    let bit = 1u8 << n;
    pm1_modify(i2c, PM1_REG_GPIO_FUNC0, two, 0)?; // 標準GPIO
    pm1_modify(i2c, PM1_REG_GPIO_PUPD0, two, 0)?; // プルなし
    pm1_modify(i2c, PM1_REG_GPIO_DRV, bit, 0)?; // push-pull
    pm1_modify(i2c, PM1_REG_GPIO_OUT, bit, if high { bit } else { 0 })?; // 先にレベル
    pm1_modify(i2c, PM1_REG_GPIO_MODE, bit, bit) // 最後に出力化（グリッチ防止）
}

/// アンプON/OFF（再生しない間はOFFにするとホワイトノイズ・待機電流が減る）
pub fn set_amp<I: I2c>(i2c: &mut I, on: bool) -> Result<(), I::Error> {
    pm1_gpio_output(i2c, PM1_GPIO_SPK, on)
}

/// 音声ハードを初期化する。`volume` は 0..=100（バッテリー駆動なら 75 以下）。
///
/// アンプは最後に ON にする（codec が立ち上がる前に ON にするとポップ音が出やすい）。
pub fn init<I: I2c, D: DelayNs>(
    i2c: &mut I,
    delay: &mut D,
    volume: u8,
) -> Result<(), AudioError<I::Error>> {
    // PM1 は I2C アイドルスリープ中だと最初の1回が失敗する（データシート記載）。
    // 結果は捨てて起こすだけ。
    let mut dummy = [0u8];
    let _ = i2c.write_read(PM1_ADDR, &[PM1_REG_GPIO_IN], &mut dummy);

    // L3B 電源ON、アンプは一旦OFFで初期化
    pm1_gpio_output(i2c, PM1_GPIO_L3B, true).map_err(AudioError::Pm1)?;
    pm1_gpio_output(i2c, PM1_GPIO_SPK, false).map_err(AudioError::Pm1)?;

    // ES8311
    let codec = Es8311::new(ES8311_ADDR);
    let clk = ClockConfig {
        mclk_inverted: false,
        sclk_inverted: false,
        mclk_from_mclk_pin: true, // MCLK は ESP32-S3 の GPIO18 から
        mclk_frequency: MCLK_HZ,
        sample_frequency: SAMPLE_RATE,
    };
    codec
        .init(i2c, &clk, Resolution::Bits16, Resolution::Bits16, delay)
        .map_err(AudioError::Codec)?;
    // init は DAC 音量を触らない（リセット直後は 0 = 無音）ので必ず設定する
    codec
        .volume_set(i2c, volume, None)
        .map_err(AudioError::Codec)?;
    codec.mute(i2c, false).map_err(AudioError::Codec)?;

    // 最後にアンプON
    set_amp(i2c, true).map_err(AudioError::Pm1)?;
    Ok(())
}
