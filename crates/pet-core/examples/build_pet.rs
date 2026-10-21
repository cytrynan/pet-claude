//! 状態ごとの静止画から、ペットのフォルダ（`pet.json`＋`spritesheet.webp`）を作る。
//!
//! ```text
//! cargo run --release -p pet-core --example build_pet -- <静止画のフォルダ> <出力フォルダ> <id> <表示名> [cellScale]
//! ```
//!
//! 静止画のファイル名は「状態の名前」を含むこと（例 `01-idle.png`）。状態の名前と、シートの
//! どの行に・どの動きで置くかは下の `PLAN`。足りない状態は、近い状態の絵で埋める（`FALLBACK`）。
//! 出力は lossless の WebP（PNG より小さい。Codex 互換の `spritesheetPath`）。

use std::path::{Path, PathBuf};

use image::RgbaImage;
use pet_core::compose::{Motion, RowSpec, compose_sheet};
use pet_core::sprite::{CELL_H, CELL_W, MAX_CELL_SCALE, Pose};

/// (状態の名前, 行, 左右反転, 動き)。行動が使うポーズは behavior.rs を参照。
const PLAN: [(&str, Pose, bool, Motion); 11] = [
    // 待機と睡眠の呼吸はアプリが描くたびに付ける（pet.json の breathe）。コマに焼き込むと 1 周 6 コマの
    // 段差になってカクつく（2026-10-21 K 指示「もう少し滑らかに」）
    ("idle", Pose::Idle, false, Motion::Still),
    ("walk", Pose::RunRight, false, Motion::Bob(0.03)),
    ("walk", Pose::RunLeft, true, Motion::Bob(0.03)),
    ("attention", Pose::Waving, false, Motion::Sway(0.015)),
    ("happy", Pose::Jumping, false, Motion::Hop(0.08)),
    ("confused", Pose::Failed, false, Motion::Sway(0.012)),
    // 入力待ち＝こちらを見て小さく手を振る（K の対応表）
    ("attention", Pose::Waiting, false, Motion::Sway(0.01)),
    // ツールを実行中＝手元で作業
    ("working", Pose::Running, false, Motion::Bob(0.01)),
    ("working", Pose::Review, false, Motion::Bob(0.01)),
    ("sleeping", Pose::Sleep, false, Motion::Still),
    ("dragged", Pose::Grabbed, false, Motion::Sway(0.02)),
];

/// 待機と睡眠の呼吸の大きさ（縦に 2.5%）。アプリがこの値で滑らかに伸び縮みさせる。
const BREATHE: f64 = 0.025;

/// 絵が無い状態の代わり（無ければ待機）。
const FALLBACK: [(&str, &str); 7] = [
    ("walk", "idle"),
    ("attention", "idle"),
    ("happy", "attention"),
    ("confused", "idle"),
    ("working", "idle"),
    ("sleeping", "idle"),
    ("dragged", "attention"),
];

fn find(dir: &Path, state: &str) -> Option<PathBuf> {
    let mut hits: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("png") || e.eq_ignore_ascii_case("webp"))
                && p.file_stem()
                    .is_some_and(|s| s.to_string_lossy().to_ascii_lowercase().contains(state))
        })
        .collect();
    hits.sort();
    hits.into_iter().next()
}

fn load(dir: &Path, state: &str) -> Result<(String, RgbaImage), String> {
    let mut name = state;
    loop {
        if let Some(p) = find(dir, name) {
            let img = image::open(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            if name != state {
                eprintln!("{state}: 絵が無いので {name} で代用");
            }
            return Ok((name.to_string(), img.to_rgba8()));
        }
        name = FALLBACK
            .iter()
            .find(|(s, _)| *s == name)
            .map(|(_, f)| *f)
            .ok_or_else(|| format!("{dir:?} に idle の絵が無い"))?;
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        return Err(
            "使い方: build_pet <静止画のフォルダ> <出力フォルダ> <id> <表示名> [cellScale]".into(),
        );
    }
    let (src, out, id, name) = (Path::new(&args[1]), Path::new(&args[2]), &args[3], &args[4]);
    // アプリは 1〜MAX_CELL_SCALE に丸めて読むので、範囲外を書くと絵と pet.json が食い違う。ここで止める
    // （2026-10-21 検証係の指摘: 8 だと 900MB 級の確保をし、アプリ側は 4 として切り出して絵が崩れた）
    let cell_scale: u32 = match args.get(5) {
        None => 2,
        Some(s) => s
            .parse()
            .ok()
            .filter(|n| (1..=MAX_CELL_SCALE).contains(n))
            .ok_or_else(|| format!("cellScale は 1〜{MAX_CELL_SCALE} の整数: {s}"))?,
    };
    let (cw, ch) = (CELL_W * cell_scale, CELL_H * cell_scale);

    let mut cache: Vec<(String, RgbaImage)> = Vec::new();
    for (state, ..) in PLAN {
        if !cache.iter().any(|(s, _)| s == state) {
            let (_, img) = load(src, state)?;
            cache.push((state.to_string(), img));
        }
    }
    let get = |s: &str| &cache.iter().find(|(n, _)| n == s).expect("読み込み済み").1;
    let specs: Vec<RowSpec<'_>> = PLAN
        .iter()
        .map(|&(state, pose, mirror, motion)| RowSpec {
            pose,
            image: get(state),
            mirror,
            motion,
        })
        .collect();
    let sheet = compose_sheet(&specs, get("idle"), 11, cw, ch);

    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let sheet_path = out.join("spritesheet.webp");
    let file = std::fs::File::create(&sheet_path).map_err(|e| e.to_string())?;
    image::codecs::webp::WebPEncoder::new_lossless(std::io::BufWriter::new(file))
        .encode(
            sheet.as_raw(),
            sheet.width(),
            sheet.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    let manifest = serde_json::json!({
        "id": id,
        "displayName": name,
        "spritesheetPath": "spritesheet.webp",
        "cellScale": cell_scale,
        "breathe": BREATHE,
    });
    let mut text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    text.push('\n');
    std::fs::write(out.join("pet.json"), text).map_err(|e| e.to_string())?;
    // 目で確かめる用の縮小見本（環境変数 PET_PREVIEW に PNG のパス。リポジトリには入れない）
    if let Some(preview) = std::env::var_os("PET_PREVIEW") {
        let small = image::imageops::resize(
            &sheet,
            sheet.width() / 4,
            sheet.height() / 4,
            image::imageops::FilterType::Triangle,
        );
        small.save(&preview).map_err(|e| e.to_string())?;
    }
    let bytes = std::fs::metadata(&sheet_path).map_or(0, |m| m.len());
    println!(
        "{} ({}x{}, 1 コマ {cw}x{ch}, {} KB)",
        sheet_path.display(),
        sheet.width(),
        sheet.height(),
        bytes / 1024
    );
    Ok(())
}
