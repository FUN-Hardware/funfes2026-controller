//! 画面描画の初期化（M5StickS3 内蔵LCD: ST7789P3 / 135x240 / SPI2）。
//!
//! 使い方:
//!   main で `audio::codec::init(..)` の**あと**に `display::init(..)` を呼び、
//!   返ってきた `Display` を display_task へ渡す。
//!
//! 順序が重要な理由: LCD(とバックライト)の電源は PM1 の L3B レール
//! （[`crate::audio::codec`] の `PM1_GPIO_L3B`）から供給されている。
//! L3B がOFFのまま初期化すると、コマンドを送っても反応しない。

use embedded_graphics::{
    mono_font::{
        MonoFont, MonoTextStyle,
        ascii::{FONT_9X18_BOLD, FONT_10X20},
    },
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle, Triangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::{
    Blocking,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    peripherals::{GPIO21, GPIO38, GPIO39, GPIO40, GPIO41, GPIO45, SPI2},
    spi::{
        Mode,
        master::{Config, Spi},
    },
    time::Rate,
};
use mipidsi::{
    Builder,
    interface::SpiInterface,
    models::ST7789,
    options::{ColorInversion, Orientation, Rotation},
};
use static_cell::StaticCell;

use crate::types::{CalibKind, CalibStatus};

/// パネルの物理サイズ。ST7789 のフレームバッファ(240x320)の一部だけを使う。
pub const PANEL_WIDTH: u16 = 135;
pub const PANEL_HEIGHT: u16 = 240;
/// 135x240 パネルがフレームバッファ内のどこに載っているか（M5GFX の StickS3 設定と同じ値）。
const OFFSET_X: u16 = 52;
const OFFSET_Y: u16 = 40;

/// 画面の向き。`Deg0` はUSB端子を下にした縦持ち(135x240)で、これがM5GFXの既定。
/// 横持ち(240x135)にしたければ `Rotation::Deg90` / `Deg270` に変える。
const ORIENTATION: Orientation = Orientation::new().rotate(Rotation::Deg180);

/// SPIクロック。M5GFX も書き込み 40MHz で駆動している。
const SPI_FREQ: Rate = Rate::from_mhz(40);

/// `SpiInterface` がピクセル送出に使う中間バッファ。
/// 大きいほどSPIトランザクション数が減る。全画面(135*240*2 = 64,800 byte)を
/// このサイズずつ分割して送る。
const SPI_BUF_LEN: usize = 4096;

type DisplaySpi = ExclusiveDevice<Spi<'static, Blocking>, Output<'static>, Delay>;

/// display_task が受け取る、初期化済みの描画先。
pub type Display =
    mipidsi::Display<SpiInterface<'static, DisplaySpi, Output<'static>>, ST7789, Output<'static>>;

/// LCDを初期化し、黒で塗りつぶしてからバックライトを点ける。
///
/// **プログラム中で1回だけ**呼ぶこと（内部で static を確保する）。
/// L3B 電源が入っている状態で呼ぶこと（モジュールのdocを参照）。
pub fn init(
    spi2: SPI2<'static>,
    sclk: GPIO40<'static>,
    mosi: GPIO39<'static>,
    cs: GPIO41<'static>,
    dc: GPIO45<'static>,
    rst: GPIO21<'static>,
    backlight: GPIO38<'static>,
) -> Display {
    static SPI_BUF: StaticCell<[u8; SPI_BUF_LEN]> = StaticCell::new();
    // Output を落とすとピンが解放されてバックライトが消えるので、'static に固定して持たせ続ける。
    static BACKLIGHT: StaticCell<Output<'static>> = StaticCell::new();

    let output_config = OutputConfig::default();
    // CSはアイドルHigh、RSTは負論理なので初期値High（mipidsiがリセットパルスを出す）。
    let cs = Output::new(cs, Level::High, output_config);
    let dc = Output::new(dc, Level::Low, output_config);
    let rst = Output::new(rst, Level::High, output_config);
    // 初期化中の画面を見せないよう、点灯は最後。
    let backlight = BACKLIGHT.init(Output::new(backlight, Level::Low, output_config));

    let spi = Spi::new(
        spi2,
        Config::default()
            .with_frequency(SPI_FREQ)
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(sclk)
    .with_mosi(mosi);

    // CS制御をSPIペリフェラルに任せず ExclusiveDevice に持たせる。
    // 4線SPIではDCの切り替えとCSの上げ下げの順序が決まっているため。
    let spi_device = ExclusiveDevice::new(spi, cs, Delay::new()).unwrap();
    let di = SpiInterface::new(spi_device, dc, SPI_BUF.init([0; SPI_BUF_LEN]));

    let mut display = Builder::new(ST7789, di)
        .reset_pin(rst)
        .display_size(PANEL_WIDTH, PANEL_HEIGHT)
        .display_offset(OFFSET_X, OFFSET_Y)
        .orientation(ORIENTATION)
        // このパネルは反転表示(INVON)前提。入れないと白黒が逆になる。
        .invert_colors(ColorInversion::Inverted)
        .init(&mut Delay::new())
        .unwrap();

    // リセット直後のVRAMは不定なので、点灯前に必ず塗り潰す。
    display.clear(Rgb565::BLACK).unwrap();
    backlight.set_high();

    display
}

/// 画面の論理サイズ。`ORIENTATION` を縦横で切り替えたらここも自動で入れ替わる。
const W: u32 = match ORIENTATION.rotation {
    Rotation::Deg0 | Rotation::Deg180 => PANEL_WIDTH as u32,
    Rotation::Deg90 | Rotation::Deg270 => PANEL_HEIGHT as u32,
};
const H: u32 = match ORIENTATION.rotation {
    Rotation::Deg0 | Rotation::Deg180 => PANEL_HEIGHT as u32,
    Rotation::Deg90 | Rotation::Deg270 => PANEL_WIDTH as u32,
};

/// キャリブレーション画面の文字。幅135pxに収めるため、
/// タイトルは13文字・説明文は14文字までしか入らない。
const TITLE_FONT: MonoFont = FONT_10X20;
const BODY_FONT: MonoFont = FONT_9X18_BOLD;
/// タイトルと説明文の間隔、説明文の行送り。
const TITLE_GAP: i32 = 24;
const LINE_H: i32 = 26;

/// 桁の周囲の余白と、桁どうしの間隔。
const MARGIN: u32 = 12;
const DIGIT_GAP: u32 = 10;

const BG: Rgb565 = Rgb565::BLACK;
const TITLE_COLOR: Rgb565 = Rgb565::WHITE;
const BODY_COLOR: Rgb565 = Rgb565::new(20, 40, 20);
/// 点灯セグメント。残弾があるときは琥珀色、0発なら赤。
const SEG_ON: Rgb565 = Rgb565::new(31, 40, 0);
const SEG_EMPTY: Rgb565 = Rgb565::new(31, 0, 0);
/// 消灯セグメント。真っ黒にせず薄く残すと電子部品の7セグらしく見える。
const SEG_OFF: Rgb565 = Rgb565::new(3, 6, 3);

/// 7セグのビットパターン。bit0=a(上) から bit6=g(中央) の順。
const SEG_PATTERN: [u8; 10] = [0x3F, 0x06, 0x5B, 0x4F, 0x66, 0x6D, 0x7D, 0x07, 0x7F, 0x6F];

/// 描画時のエラー（実体はSPIの失敗）。
pub type DrawError = <Display as DrawTarget>::Error;

/// 7セグ1桁の寸法。縦横比は 1:2 固定。
struct DigitSize {
    /// セグメントの太さの半分。寸法はすべてこれから決まる。
    half: u32,
}

impl DigitSize {
    /// セグメントの太さ。奇数にすると先端の斜辺がちょうど45°になり、
    /// 隣のセグメントの頂点と1点でぴったり噛み合う。
    const fn thickness(&self) -> u32 {
        self.half * 2 + 1
    }

    /// 桁の幅と高さ。縦セグメントの長さを揃えるため高さは奇数にする。
    const fn width(&self) -> u32 {
        self.half * 12 + 7
    }

    const fn height(&self) -> u32 {
        self.half * 24 + 15
    }

    /// 隣のセグメントとの間隔。頂点をこの分だけ内側へ引っ込めるので、
    /// 先端どうしは斜めに `gap * √2` ほど離れる。
    const fn gap(&self) -> i32 {
        (self.half / 3 + 1) as i32
    }

    /// `digits` 桁が画面に収まる最大の寸法。高さと幅の両方の制約から太さを決める。
    const fn fit(digits: u32) -> Self {
        let by_height = (H - MARGIN * 2 - 15) / 24;
        let by_width = ((W - MARGIN * 2 - DIGIT_GAP * (digits - 1)) / digits - 7) / 12;
        let half = if by_height <= by_width {
            by_height
        } else {
            by_width
        };
        Self {
            half: if half < 1 { 1 } else { half },
        }
    }
}

/// 表示する内容を決めて描く係。前回の内容を覚えていて、変わった部分だけ描き直す。
pub struct Screen {
    display: Display,
    last: Option<(CalibStatus, u8)>,
}

impl Screen {
    pub const fn new(display: Display) -> Self {
        Self {
            display,
            last: None,
        }
    }

    /// 現在のキャリブレーション状態と残弾数を画面に反映する。
    pub fn render(&mut self, calib: CalibStatus, ammo: u8) -> Result<(), DrawError> {
        let calib_changed = !matches!(self.last, Some((last, _)) if last == calib);
        // 桁数が変わると桁の寸法も位置も変わるので、消してから描き直す。
        let digits_changed =
            matches!(self.last, Some((_, last)) if digit_count(last) != digit_count(ammo));

        if calib_changed || digits_changed {
            self.display.clear(BG)?;
            self.draw_labels(calib)?;
        }

        if calib == CalibStatus::Idle {
            self.draw_ammo(ammo)?;
        }

        self.last = Some((calib, ammo));
        Ok(())
    }

    /// 状態ごとのタイトルと操作説明。`CalibStatus` がそのまま画面の種類になる。
    fn draw_labels(&mut self, calib: CalibStatus) -> Result<(), DrawError> {
        let (title, body) = match calib {
            // 残弾の画面は7セグだけ。文字は出さない。
            CalibStatus::Idle => return Ok(()),
            CalibStatus::Selecting => (
                "CALIB",
                &["SELECT MODE", "TAP : CORNERS", "HOLD: STILL"][..],
            ),
            CalibStatus::Running(CalibKind::Stationary) => {
                ("STILL", &["KEEP STILL", "DO NOT MOVE"][..])
            }
            CalibStatus::Running(CalibKind::Orientation) => {
                ("CORNERS", &["AIM CORNER", "PULL TRIGGER", "x4"][..])
            }
        };

        let title_style = MonoTextStyle::new(&TITLE_FONT, TITLE_COLOR);
        let body_style = MonoTextStyle::new(&BODY_FONT, BODY_COLOR);
        // 座標を文字の上端で扱えるようにして、行数に応じた縦中央寄せを素直に書く。
        let layout = TextStyleBuilder::new()
            .alignment(Alignment::Center)
            .baseline(Baseline::Top)
            .build();

        let title_h = TITLE_FONT.character_size.height as i32;
        let body_h = BODY_FONT.character_size.height as i32;
        let block_h = title_h + TITLE_GAP + LINE_H * (body.len() as i32 - 1) + body_h;
        let top = (H as i32 - block_h) / 2;
        let center = W as i32 / 2;

        Text::with_text_style(title, Point::new(center, top), title_style, layout)
            .draw(&mut self.display)?;

        let body_top = top + title_h + TITLE_GAP;
        for (i, line) in body.iter().enumerate() {
            Text::with_text_style(
                line,
                Point::new(center, body_top + LINE_H * i as i32),
                body_style,
                layout,
            )
            .draw(&mut self.display)?;
        }

        Ok(())
    }

    /// 残弾数を7セグで画面中央に描く。消灯セグメントも塗るので事前の消去は要らない。
    fn draw_ammo(&mut self, ammo: u8) -> Result<(), DrawError> {
        let digits = digit_count(ammo);
        let size = DigitSize::fit(digits);

        let total_w = size.width() * digits + DIGIT_GAP * (digits - 1);
        let left = (W as i32 - total_w as i32) / 2;
        let top = (H as i32 - size.height() as i32) / 2;
        let color = if ammo == 0 { SEG_EMPTY } else { SEG_ON };

        for i in 0..digits {
            // 上の桁から順に。1桁なら ammo % 10 だけを描くことになる。
            let value = ammo / 10u8.pow(digits - 1 - i) % 10;
            let x = left + (size.width() + DIGIT_GAP) as i32 * i as i32;
            self.draw_digit(value, Point::new(x, top), &size, color)?;
        }

        Ok(())
    }

    fn draw_digit(
        &mut self,
        value: u8,
        origin: Point,
        size: &DigitSize,
        on: Rgb565,
    ) -> Result<(), DrawError> {
        let half = size.half as i32;
        let (w, h) = (size.width() as i32, size.height() as i32);
        let g = size.gap();

        // 頂点を置く格子。隣り合うセグメントの先端はこの格子点を向かい合って共有する。
        let (left, right) = (half, w - 1 - half);
        let (top, middle, bottom) = (half, (h - 1) / 2, h - 1 - half);

        // 各セグメントを先端2点で表す。a, b, c, d, e, f, g の順
        // （SEG_PATTERN のビット順と揃えている）。格子点から長さ方向に `g` だけ
        // 引っ込めることで、隣のセグメントとの間に隙間ができる。
        let segments = [
            ((left + g, top), (right - g, top)),        // a: 上
            ((right, top + g), (right, middle - g)),    // b: 右上
            ((right, middle + g), (right, bottom - g)), // c: 右下
            ((left + g, bottom), (right - g, bottom)),  // d: 下
            ((left, middle + g), (left, bottom - g)),   // e: 左下
            ((left, top + g), (left, middle - g)),      // f: 左上
            ((left + g, middle), (right - g, middle)),  // g: 中央
        ];

        let pattern = SEG_PATTERN[value as usize];
        for (i, &((x1, y1), (x2, y2))) in segments.iter().enumerate() {
            let color = if pattern & (1 << i) != 0 { on } else { SEG_OFF };
            self.segment(
                origin + Point::new(x1, y1),
                origin + Point::new(x2, y2),
                size,
                color,
            )?;
        }

        Ok(())
    }

    /// セグメント1本を、両端が尖った六角形（矩形＋三角形2枚）で描く。
    /// `from`/`to` は尖った先端の座標そのもので、隣のセグメントとはこの点で接する。
    fn segment(
        &mut self,
        from: Point,
        to: Point,
        size: &DigitSize,
        color: Rgb565,
    ) -> Result<(), DrawError> {
        let half = size.half as i32;
        let thickness = size.thickness();
        // 三角形の底辺は矩形の端の列と重ねる（1pxの切れ目が出ないように）。
        let body_len = (to.x - from.x + to.y - from.y) as u32 - thickness + 1;
        let horizontal = from.y == to.y;

        let (body_origin, body_size, from_base, to_base) = if horizontal {
            let y = from.y - half;
            (
                Point::new(from.x + half, y),
                Size::new(body_len, thickness),
                (
                    Point::new(from.x + half, y),
                    Point::new(from.x + half, y + thickness as i32 - 1),
                ),
                (
                    Point::new(to.x - half, y),
                    Point::new(to.x - half, y + thickness as i32 - 1),
                ),
            )
        } else {
            let x = from.x - half;
            (
                Point::new(x, from.y + half),
                Size::new(thickness, body_len),
                (
                    Point::new(x, from.y + half),
                    Point::new(x + thickness as i32 - 1, from.y + half),
                ),
                (
                    Point::new(x, to.y - half),
                    Point::new(x + thickness as i32 - 1, to.y - half),
                ),
            )
        };

        self.fill(body_origin, body_size, color)?;
        self.triangle(from, from_base.0, from_base.1, color)?;
        self.triangle(to, to_base.0, to_base.1, color)
    }

    fn fill(&mut self, position: Point, size: Size, color: Rgb565) -> Result<(), DrawError> {
        Rectangle::new(position, size)
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(&mut self.display)
    }

    fn triangle(&mut self, a: Point, b: Point, c: Point, color: Rgb565) -> Result<(), DrawError> {
        Triangle::new(a, b, c)
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(&mut self.display)
    }
}

/// 7セグで何桁必要か。0は1桁扱い。
fn digit_count(ammo: u8) -> u32 {
    if ammo >= 100 {
        3
    } else if ammo >= 10 {
        2
    } else {
        1
    }
}
