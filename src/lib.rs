#![no_std]

/// JSON出力(先頭が `{`/`[`)と混同しないよう `# ` を付けてデバッグ出力する。
/// releaseビルド(`debug_assertions`無効)では引数の評価だけ残して出力自体は消える。
#[macro_export]
macro_rules! debug_println {
    ($($arg:tt)*) => {{
        #[cfg(debug_assertions)]
        {
            ::esp_println::println!("# {}", format_args!($($arg)*));
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = format_args!($($arg)*);
        }
    }};
}

pub mod game;
pub mod gyro;
pub mod input;
pub mod output;
pub mod types;
