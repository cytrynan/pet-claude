//! `%LOCALAPPDATA%\pet-claude\logs\pet.log` への 1 行ログ。検証の証拠にも使う。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
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
