//! アプリのアイコンを書き出す。リポジトリの直下で実行する:
//! `cargo run -p pet-core --example gen_assets`
//!
//! 以前は内蔵の仮キャラ（`assets/pets/placeholder`）もここで作っていたが、2026-10-28 K 指示で仮キャラを
//! 外した。アイコンは仮キャラの絵（`pet_core::placeholder`）から作ったままにする。

use std::fs;
use std::path::Path;

use pet_core::placeholder::generate_icon;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let icon_dir = Path::new("crates/pet-app/icons");
    fs::create_dir_all(icon_dir)?;
    generate_icon(256).save(icon_dir.join("icon.png"))?;
    generate_icon(64).save(icon_dir.join("icon.ico"))?;
    println!("wrote {}", icon_dir.display());
    Ok(())
}
