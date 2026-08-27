# funfes2026-controller

大学祭（未来祭2026）で展示予定のシューティングゲーム向けコントローラーの、組み込み側（ファームウェア）リポジトリです。

M5Stack StickS3 上で動作し、内蔵ジャイロセンサーによる照準操作とトリガー/リロード入力を読み取り、USBシリアル経由でPC(Unity)側に送信します。PC側の受信・ゲーム本体はこのリポジトリには含まれません。

## 出力

コントローラーからPC(Unity)へ、USBシリアル経由でJSON（改行区切り、1行1メッセージ）を一方向に送信します。ACK等のハンドシェイクはありません。

想定している内容:

- ジャイロ由来の照準角度
- トリガー / リロードの状態
- 残弾数（コントローラー側でカウント）

具体的なフィールド名・型などのメッセージ仕様は **未確定** です。

## 使用技術

- 言語: [Rust](https://www.rust-lang.org/)（`no_std` / `no_main`）
- マイコン: M5Stack StickS3 (ESP32-S3-PICO-1-N8R8 / IMU: BMI270)
- HAL: [esp-hal](https://github.com/esp-rs/esp-hal)
- 実行環境: [esp-rtos](https://github.com/esp-rs/esp-hal)（embassy executor）
- IMUドライバ: [`bmi2`](https://crates.io/crates/bmi2)

## 使い方

### 必要環境

- ESP32-S3向けのRustツールチェーン（[esp-rs](https://github.com/esp-rs/rust-build)。`rust-toolchain.toml` で `channel = "esp"` を指定済み）
- [`espflash`](https://github.com/esp-rs/espflash)

### ビルド

```bash
cargo build --release
```

### 書き込み・実行

```bash
cargo run --release
```

`.cargo/config.toml` で `espflash flash --monitor` がランナーとして設定されているため、ビルド後に自動で書き込み・シリアルモニタ起動まで行われます。

### ジャイロ動作確認用バイナリ

ジャイロの読み取り値をシリアル出力で確認できる単体テスト的なバイナリです。

```bash
cargo run --bin gyro
```

## ハードウェア

- マイコン: M5Stack StickS3
- 入力: マイクロスイッチ（トリガー・リロード用）
- 基板・筐体: **未定**

---

# タスク設計

```mermaid
flowchart TD
    subgraph input["入力"]
        trigger_task
        reload_task
        calib_button_task
    end

    subgraph core["中核"]
        gyro_task
        trigger_router_task
        game_state_task
    end

    subgraph output["出力"]
        display_task
        json_output_task
        sound_task
    end

    trigger_task -- "()" --> trigger_ch(["Channel&lt;()&gt;"])
    trigger_ch --> trigger_router_task

    calib_button_task -- "CalibStatus" --> calib_watch(["Watch&lt;CalibStatus&gt;"])
    calib_watch --> gyro_task
    calib_watch --> trigger_router_task

    gyro_task -- "(pitch, yaw)" --> gyro_watch(["Watch&lt;(f32, f32)&gt;"])
    gyro_watch --> trigger_router_task
    gyro_watch --> json_output_task

    trigger_router_task -- "((pitch_min, pitch_max), (yaw_min, yaw_max))" --> calib_range_watch(["Watch&lt;((f32, f32), (f32, f32))&gt;"])
    calib_range_watch --> json_output_task

    trigger_router_task -- "GameEvent::Fired" --> game_event(["Channel&lt;GameEvent&gt;"])
    reload_task -- "GameEvent::Reloaded" --> game_event
    game_event --> game_state_task

    trigger_router_task -. "四隅取得完了時に CalibStatus::Idle" .-> calib_watch

    game_state_task -- "GameState" --> game_state(["Watch&lt;GameState&gt;"])
    game_state --> display_task
    game_state --> json_output_task

    game_state_task -- "SoundEvent" --> sound_event(["Channel&lt;SoundEvent&gt;"])
    sound_event --> sound_task
```

トリガー入力の意味（発砲 / キャリブレーション操作）は `CalibStatus` によって変わるため、`trigger_task` からの入力は `trigger_router_task` が一箇所で受け、現在の `CalibStatus` を見て振り分ける:

- `Idle`: `GameEvent::Fired` を送出（通常の発砲）
- `Running(Orientation)`: その時点のジャイロ角度を画面四隅の1点として記録し、4点集まったら pitch/yaw それぞれの `(min, max)` を `calib_range_watch` に送出したうえで `CalibStatus::Idle` に戻す
- それ以外（`Selecting` / `Running(Stationary)`）: 無視

`display_task` / `sound_task` は残弾数・状態や発射/リロードイベントにのみ反応するため、ジャイロ角度・キャリブレーション範囲は購読しない。角度の0〜1正規化は `json_output_task` が自身の出力タイミングでのみ、`gyro_watch` の最新角度と `calib_range_watch` の `(min, max)` から都度計算する（ジャイロの取得間隔ごとに計算し続けることはしない）。
