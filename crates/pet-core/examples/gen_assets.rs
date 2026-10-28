//! 仮キャラの絵からアイコンを書き出す（確認用）。リポジトリの直下で実行する:
//! `cargo run -p pet-core --example gen_assets` → `target/gen-assets/icon.png` と `icon.ico`
//!
//! 以前は内蔵の仮キャラ（`assets/pets/placeholder`）もここで作っていたが、2026-10-28 K 指示で仮キャラを
//! 外した。アプリの本当のアイコン（`crates/pet-app/icons/`）は 2026-10-28 からまめドラゴンの待機の 1 コマ目で、
//! `python -I scripts/make-icon.py assets/pets/mame-dragon/spritesheet.webp crates/pet-app/icons` で作る。
//! ここでは本物を上書きしないように `target/` の下へ書く。

use std::fs;
use std::path::Path;

use pet_core::placeholder::generate_icon;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let icon_dir = Path::new("target/gen-assets");
    fs::create_dir_all(icon_dir)?;
    generate_icon(256).save(icon_dir.join("icon.png"))?;
    generate_icon(64).save(icon_dir.join("icon.ico"))?;
    println!("wrote {}", icon_dir.display());
    Ok(())
}
