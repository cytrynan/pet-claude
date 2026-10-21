//! `%LOCALAPPDATA%\pet-claude\logs\pet.log` への 1 行ログ。検証の証拠にも使う。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// 状態・ログ・発見ファイルの置き場所。`PET_CLAUDE_DATA_DIR` で差し替えられる（試験用。pet-hook と同じ規則）。
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PET_CLAUDE_DATA_DIR") {
        return PathBuf::from(d);
    }
    let base = std::env::var_os("LOCALAPPDATA").map_or_else(std::env::temp_dir, PathBuf::from);
    base.join("pet-claude")
}

pub fn log(msg: impl AsRef<str>) {
    let dir = data_dir().join("logs");
    let _ = fs::create_dir_all(&dir);
    let line = format!(
        "{} {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
        msg.as_ref()
    );
    if cfg!(debug_assertions) {
        eprint!("{line}");
    }
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("pet.log"))
    {
        let _ = f.write_all(line.as_bytes());
    }
}
