//! 効果音再生（esp-hal I2S + DMA、async）。
//!
//! 使い方:
//!   main で `codec::init(&mut i2c, &mut delay, 60)` → `init_i2s(...)` → `spawner.spawn(sound_task(..))`
//!   他タスクから `SoundEvent` を SOUND_EVENT_CHANNEL に送って再生要求。

pub mod codec;

use crate::types::SoundEvent;
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel};
use esp_hal::{
    Async, dma_buffers,
    i2s::master::{Channels, Config, DataFormat, I2s, I2sTx},
    peripherals::{DMA_CH0, GPIO14, GPIO15, GPIO17, GPIO18, I2S0},
    time::Rate,
};

/// DMAバッファ長。1回の転送で送れる最大バイト数。
/// 16kHz/16bit/モノラル = 32000 byte/s なので 4092*8 = 32736 byte ≒ 1.02秒分。
pub const DMA_BUF_LEN: usize = 4092 * 8;

/// `assets/shot.wav`: `ffmpeg -i in.mp3 -ac 1 -ar 16000 -sample_fmt s16 -map_metadata -1 -f wav shot.wav`
static SHOT_WAV: &[u8] = include_bytes!("../../assets/shot.wav");

/// I2S(TX) を初期化する。**プログラム中で1回だけ**呼ぶこと（内部の dma_buffers! が static を確保する）。
///
/// MCLK(GPIO18) はここで出力設定される。codec::init より前に呼んでおくと安全。
pub fn init_i2s(
    i2s0: I2S0<'static>,
    dma: DMA_CH0<'static>,
    mclk: GPIO18<'static>,
    bclk: GPIO17<'static>,
    ws: GPIO15<'static>,
    dout: GPIO14<'static>,
) -> (I2sTx<'static, Async>, &'static mut [u8]) {
    let (_, _, tx_buffer, tx_descriptors) = dma_buffers!(0, DMA_BUF_LEN);

    let i2s = I2s::new(
        i2s0,
        dma,
        Config::new_tdm_philips()
            .with_sample_rate(Rate::from_hz(codec::SAMPLE_RATE))
            .with_data_format(DataFormat::Data16Channel16)
            // 同じデータを左右両ch に出す。wav はモノラルのままでOK。
            .with_channels(Channels::MONO),
    )
    .unwrap()
    .with_mclk(mclk)
    .into_async();

    let tx = i2s
        .i2s_tx
        .with_bclk(bclk)
        .with_ws(ws)
        .with_dout(dout)
        .build(tx_descriptors);

    // dma_buffers! は &'static mut [u8; N] を返すのでスライスに揃える
    let tx_buffer: &'static mut [u8] = tx_buffer;
    (tx, tx_buffer)
}

/// RIFF/WAVE から "data" チャンクの中身（PCM）だけを取り出す。
/// ヘッダ長は固定ではない（LIST 等が挟まる）ので、チャンクを辿って探す。
fn wav_pcm(wav: &[u8]) -> &[u8] {
    let mut i = 12; // "RIFF" + size + "WAVE"
    while i + 8 <= wav.len() {
        let id = &wav[i..i + 4];
        let size = u32::from_le_bytes([wav[i + 4], wav[i + 5], wav[i + 6], wav[i + 7]]) as usize;
        let body = i + 8;
        if id == b"data" {
            return &wav[body..(body + size).min(wav.len())];
        }
        i = body + size + (size & 1); // チャンクは偶数境界
    }
    &[]
}

/// PCM を DMA バッファに詰めて送出する。終端に短い無音を送って出力を0に戻す。
async fn play(tx: &mut I2sTx<'static, Async>, buf: &mut [u8], wav: &[u8]) {
    for chunk in wav_pcm(wav).chunks(buf.len()) {
        let n = chunk.len();
        let n4 = (n + 3) & !3; // 4byte 境界に切り上げ（余りは無音）
        buf[..n].copy_from_slice(chunk);
        buf[n..n4].fill(0);
        if let Err(e) = tx.write_dma_async(&mut buf[..n4]).await {
            esp_println::println!("i2s write error: {:?}", e);
            return;
        }
    }
    buf[..256].fill(0);
    let _ = tx.write_dma_async(&mut buf[..256]).await;
}

#[embassy_executor::task]
pub async fn sound_task(
    mut tx: I2sTx<'static, Async>,
    buf: &'static mut [u8],
    sound_event_receiver: channel::Receiver<'static, CriticalSectionRawMutex, SoundEvent, 3>,
) {
    let mut event = sound_event_receiver.receive().await;
    loop {
        let wav = match event {
            SoundEvent::Fire => SHOT_WAV,
            // リロード音は未実装（wav が用意できたらここで再生する）
            SoundEvent::Reload => {
                event = sound_event_receiver.receive().await;
                continue;
            }
        };

        // 再生中に新しいイベントが来たら、再生を打ち切ってそのイベントを最初から再生し直す
        event = match select(play(&mut tx, buf, wav), sound_event_receiver.receive()).await {
            Either::First(()) => sound_event_receiver.receive().await,
            Either::Second(next) => next,
        };
    }
}
