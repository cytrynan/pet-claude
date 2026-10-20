//! 仮画像とアイコンを書き出す。リポジトリの直下で実行する:
//! `cargo run -p pet-core --example gen_assets`

use std::fs;
use std::path::Path;

use pet_core::placeholder::{generate_icon, generate_sheet};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pet_dir = Path::new("assets/pets/placeholder");
    fs::create_dir_all(pet_dir)?;
    generate_sheet().save(pet_dir.join("spritesheet.png"))?;
    fs::write(
        pet_dir.join("pet.json"),
        "{\n  \"id\": \"placeholder\",\n  \"displayName\": \"仮キャラ\",\n  \"spritesheetPath\": \"spritesheet.png\"\n}\n",
    )?;

    let icon_dir = Path::new("crates/pet-app/icons");
    fs::create_dir_all(icon_dir)?;
    generate_icon(256).save(icon_dir.join("icon.png"))?;
    generate_icon(64).save(icon_dir.join("icon.ico"))?;
    println!("wrote {} and {}", pet_dir.display(), icon_dir.display());
    Ok(())
}
