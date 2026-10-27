//! ペットの一覧と読み込み（設定画面で切り替える。K 指示 2026-10-21）。
//!
//! - 内蔵の仮キャラ（`placeholder`）は exe に埋め込む。ほかはペットのフォルダ（`pet.json`＋シート）
//! - フォルダを探す場所（先に見つかった id を使う）: `%LOCALAPPDATA%\pet-claude\pets\<id>\`、
//!   exe と同じフォルダの `pets\<id>\`、デバッグビルドだけリポジトリの `assets\pets\<id>\`
//! - 高精細のシート（`cellScale` 2 の 384×416 のコマなど）はそのまま持つと重い（まめドラゴンは
//!   展開して 56MB）。読み込んだらすぐ、いまの表示に要る解像度までコマごとに縮めて持つ
//!   （PRD N1: 普段の専有 50MB）。もっと大きく表示するときに読み直す

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use image::RgbaImage;
use pet_core::clips::sheet_poses;
use pet_core::compose::resize_clean;
use pet_core::sprite::{CELL_H, CELL_W, COLUMNS, PetManifest, Pose, SheetLayout};

use crate::clipstore::ClipPack;
use crate::log::{data_dir, log};

pub const BUILTIN_ID: &str = "placeholder";
const BUILTIN_NAME: &str = "仮キャラ";
const BUILTIN_SHEET: &[u8] = include_bytes!("../../../assets/pets/placeholder/spritesheet.png");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetEntry {
    pub id: String,
    pub name: String,
    /// ペットのフォルダ。None は内蔵の仮キャラ
    pub dir: Option<PathBuf>,
    /// 呼びかけの言葉（pet.json の `wakeWord`。無いペットと内蔵の仮キャラは None）
    pub wake_word: Option<String>,
}

/// 中身のあるコマだけを、不透明な範囲で切り抜いて持つ。シートを丸ごと持つと、空のコマ（まめドラゴンは
/// 88 コマ中 31）と透明な余白の分まで抱える。2026-10-21、K の大きさ 105%（200% の画面で 1 コマ 404×437 が要る）
/// でシートを縮めずに持ち、専有 58.6MB と PRD N1（50MB）を超えた
pub struct CellStore {
    /// 1 コマの画素数（縮めた後）
    pub cell: (u32, u32),
    /// (行, コマ) → (コマの中での左上, 切り抜いた絵)
    cells: HashMap<(u32, u32), (u32, u32, RgbaImage)>,
}

impl CellStore {
    /// シートの中身のあるコマ（`layout` の数だけ）を、`cell` の大きさに縮めてから切り抜く。
    /// `rows` を渡したらその行だけ持つ（動画クリップのあるペットは、クリップで描かない行だけで足りる）。
    fn build(
        sheet: &RgbaImage,
        source_cell: (u32, u32),
        cell: (u32, u32),
        layout: &SheetLayout,
        rows: Option<&[u32]>,
    ) -> Self {
        let mut cells = HashMap::new();
        for (row, &n) in layout.frames.iter().enumerate() {
            let row = row as u32;
            if rows.is_some_and(|r| !r.contains(&row)) {
                continue;
            }
            for col in 0..n {
                let src = image::imageops::crop_imm(
                    sheet,
                    col * source_cell.0,
                    row * source_cell.1,
                    source_cell.0,
                    source_cell.1,
                )
                .to_image();
                let img = if cell == source_cell {
                    src
                } else {
                    resize_clean(&src, cell.0, cell.1)
                };
                if let Some((x, y, w, h)) = visible_bbox(&img) {
                    cells.insert(
                        (row, col),
                        (x, y, image::imageops::crop_imm(&img, x, y, w, h).to_image()),
                    );
                }
            }
        }
        Self { cell, cells }
    }

    /// 1 コマを元の大きさのキャンバスに戻す（持っていないコマは透明）。
    pub fn cell_image(&self, row: u32, frame: u32) -> RgbaImage {
        let mut canvas = RgbaImage::new(self.cell.0, self.cell.1);
        if let Some((x, y, img)) = self.cells.get(&(row, frame)) {
            image::imageops::replace(&mut canvas, img, i64::from(*x), i64::from(*y));
        }
        canvas
    }

    /// 持っている画素のバイト数（ログと試験用）。
    pub fn bytes(&self) -> usize {
        self.cells
            .values()
            .map(|(_, _, img)| img.as_raw().len())
            .sum()
    }
}

/// アルファが 0 でない画素の範囲（縁のかすかなにじみも残す）。
fn visible_bbox(img: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] > 0 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x0 != u32::MAX).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

/// 読み込んだシート。`cell` は 1 コマの画素数（縮めた後）、`source_cell` は元のシートのコマ。
/// `layout` は行ごとの中身のあるコマ数（縮める前の元のシートで測る）。
pub struct LoadedSheet {
    pub cells: CellStore,
    pub cell: (u32, u32),
    pub source_cell: (u32, u32),
    pub layout: SheetLayout,
    /// 待機と睡眠の呼吸の大きさ（pet.json の `breathe`。内蔵の仮キャラは 0）
    pub breathe: f32,
    /// 動画クリップ（pet.json の `clips`。無ければシートだけで動く）
    pub clips: Option<ClipPack>,
}

fn roots() -> Vec<PathBuf> {
    let mut roots = vec![data_dir().join("pets")];
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        roots.push(dir.join("pets"));
    }
    if cfg!(debug_assertions) {
        roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/pets"));
    }
    roots
}

fn read_manifest(dir: &Path) -> Result<PetManifest, String> {
    let text =
        std::fs::read_to_string(dir.join("pet.json")).map_err(|e| format!("pet.json: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("pet.json: {e}"))
}

/// `roots` の下のペットのフォルダを集める。内蔵の仮キャラが先頭。同じ id は先に見つかった方。
/// 読めない pet.json はログに残して飛ばす。並びはフォルダ名の順（同じ場所の中で）。
pub fn discover_in(roots: &[PathBuf]) -> Vec<PetEntry> {
    let mut pets = vec![PetEntry {
        id: BUILTIN_ID.into(),
        name: BUILTIN_NAME.into(),
        dir: None,
        wake_word: None,
    }];
    for root in roots {
        let Ok(read) = std::fs::read_dir(root) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = read
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            match read_manifest(&dir) {
                Ok(m) if pets.iter().any(|p| p.id == m.id) => {}
                Ok(m) => pets.push(entry_from(m, dir)),
                Err(e) if dir.join("pet.json").exists() => log(format!("{}: {e}", dir.display())),
                Err(_) => {}
            }
        }
    }
    pets
}

pub fn discover() -> Vec<PetEntry> {
    discover_in(&roots())
}

/// 起動時に試す順: 選んだペット（state.json の `pet`）→ フォルダにあるペット（見つかった順）→ 内蔵の仮キャラ。
/// 起動側は読めないものを飛ばして次を試す（仮キャラはクリックの抜けを試すための絵なので最後の手段）。
pub fn startup_order<'a>(pets: &'a [PetEntry], chosen: Option<&str>) -> Vec<&'a PetEntry> {
    let first = chosen.and_then(|id| pets.iter().find(|p| p.id == id));
    let mut order: Vec<&PetEntry> = first.into_iter().collect();
    for p in pets
        .iter()
        .filter(|p| p.dir.is_some())
        .chain(pets.iter().filter(|p| p.dir.is_none()))
    {
        if !order.iter().any(|q| q.id == p.id) {
            order.push(p);
        }
    }
    order
}

/// フォルダを直接指定して開く（起動引数 `--pet <dir>`）。
pub fn entry_for_dir(dir: &Path) -> Result<PetEntry, String> {
    Ok(entry_from(read_manifest(dir)?, dir.to_path_buf()))
}

fn entry_from(m: PetManifest, dir: PathBuf) -> PetEntry {
    PetEntry {
        wake_word: m.wake_word().map(str::to_owned),
        id: m.id,
        name: m.display_name,
        dir: Some(dir),
    }
}

/// 持つコマの解像度の上限。動画のあるペットは元のコマの 3/4（それより大きい窓には描く瞬間に引き伸ばす）。
/// 2026-10-21 実測: 元の大きさで持つと、大きさ 105%・200% の画面で動画のコマ（待機 17MB・つまみ中 9MB・
/// 歩き 15MB）とシートの行（10MB）で、散歩中の専有が 66.7MB と PRD N1（50MB）を超えた。
/// 読み直しの判定（`pet_window::kick_load`）もこの上限を使う（使わないと「もっと細かく」が成り立ち続け、
/// シートを読み直し続けた。2026-10-21 実測で CPU 200%）
pub fn max_cell(source_cell: (u32, u32), has_clips: bool) -> (u32, u32) {
    max_cell_with(source_cell, has_clips, full_cell_for_tests())
}

/// `max_cell` の本体。`full` なら動画のあるペットでも元のコマの大きさまで持つ
/// （2026-10-27 K 指示の画質の試し。メモリと見え方を 3/4 と比べるため）。
pub fn max_cell_with(source_cell: (u32, u32), has_clips: bool, full: bool) -> (u32, u32) {
    if has_clips && !full {
        (source_cell.0 * 3 / 4, source_cell.1 * 3 / 4)
    } else {
        source_cell
    }
}

/// 試験用（デバッグビルドで、置き場所を `PET_CLAUDE_DATA_DIR` で差し替えた時だけ）: `PET_TEST_FULL_CELL=1` なら
/// 動画のコマを 3/4 に縮めず元の大きさで持つ。同じビルドで 3/4 と等倍を並べて測るため（2026-10-27）
fn full_cell_for_tests() -> bool {
    cfg!(debug_assertions)
        && std::env::var_os("PET_CLAUDE_DATA_DIR").is_some()
        && std::env::var_os("PET_TEST_FULL_CELL").is_some_and(|v| v == "1")
}

/// 表示倍率 `factor`（= 大きさ × DPI / 96 の最大）で 1 コマを描くのに要る画素数。
pub fn needed_cell(factor: f32) -> (u32, u32) {
    (
        (CELL_W as f32 * factor).ceil() as u32,
        (CELL_H as f32 * factor).ceil() as u32,
    )
}

/// シートを読み、`needed` より大きいコマは `needed` までコマごとに縮める（コマの境目をまたいで
/// 補間しないように 1 コマずつ）。
pub fn load(entry: &PetEntry, needed: (u32, u32)) -> Result<LoadedSheet, String> {
    let started = std::time::Instant::now();
    let (image, source_cell, breathe, clips) = match &entry.dir {
        None => (
            image::load_from_memory(BUILTIN_SHEET)
                .map_err(|e| e.to_string())?
                .into_rgba8(),
            (CELL_W, CELL_H),
            0.0,
            None,
        ),
        Some(dir) => {
            let m = read_manifest(dir)?;
            if !pet_core::sprite::is_inside_relative(&m.spritesheet_path) {
                return Err(format!(
                    "spritesheetPath はペットのフォルダの中の相対パスにする: {}",
                    m.spritesheet_path
                ));
            }
            let img = image::open(dir.join(&m.spritesheet_path))
                .map_err(|e| format!("{}: {e}", m.spritesheet_path))?
                // 複製せずに取り出す（展開した 56MB をもう 1 つ作らない。2026-10-21 検証係の指摘）
                .into_rgba8();
            // クリップが壊れていても、シートだけで動けるのでペットは読み込む（ログに残す）
            let clips = m
                .clips
                .as_deref()
                .and_then(|rel| match ClipPack::load(dir, rel) {
                    Ok(pack) => Some(pack),
                    Err(e) => {
                        log(format!("{}: クリップを使わない: {e}", dir.display()));
                        None
                    }
                });
            (img, m.cell_size(), m.breathe(), clips)
        }
    };
    let rows = image.height() / source_cell.1;
    if rows == 0 || image.width() < source_cell.0 * COLUMNS {
        return Err(format!(
            "シートが小さすぎる: {}x{}（1 コマ {}x{}・8 列）",
            image.width(),
            image.height(),
            source_cell.0,
            source_cell.1
        ));
    }
    // 測るのは縮める前の元のシートで（縮めると、ごく小さい絵だけのコマが空に見えて数え止まりうる。
    // 2026-10-21 検証係の指摘）。最初の不透明な画素で打ち切るので、元の大きさでも軽い
    let layout = SheetLayout::measure(&image, source_cell);
    if layout.is_empty() {
        return Err("中身のあるコマが 1 つも無い".into());
    }
    let max = max_cell(source_cell, clips.is_some());
    let cell = (needed.0.min(max.0), needed.1.min(max.1));
    // クリップがあるなら、シートで描くのはクリップで描けないポーズ（困る・ループの無い姿勢）の行だけ
    let rows: Option<Vec<u32>> = clips.as_ref().map(|pack| {
        // 待機の行は、最初のクリップを読み終えるまでの 1 枚に使う（無いと起動直後の窓が透明になる）
        let mut r: Vec<u32> = sheet_poses(&pack.set)
            .into_iter()
            .chain([Pose::Idle])
            .map(|p| p.clip_in(&layout).row)
            .collect();
        r.sort_unstable();
        r.dedup();
        r
    });
    let cells = CellStore::build(&image, source_cell, cell, &layout, rows.as_deref());
    // 元のシート（まめドラゴンで 56MB）はここで手放す。切り抜いたコマだけを持って戻る
    drop(image);
    log(format!(
        "pet '{}' ({}) loaded in {} ms: {} rows (frames {:?}), cell {}x{} (source {}x{}), {} KB, clips {}, sheet rows kept {:?}",
        entry.name,
        entry.id,
        started.elapsed().as_millis(),
        layout.rows(),
        layout.frames,
        cell.0,
        cell.1,
        source_cell.0,
        source_cell.1,
        cells.bytes() / 1024,
        clips.as_ref().map_or(0, |p| p.set.clips.len()),
        rows
    ));
    Ok(LoadedSheet {
        cells,
        cell,
        source_cell,
        layout,
        breathe,
        clips,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pet-claude-pets-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_pet(root: &Path, folder: &str, id: &str, name: &str, cell_scale: u32, rows: u32) {
        let dir = root.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = format!(
            r#"{{"id":"{id}","displayName":"{name}","spritesheetPath":"sheet.png","cellScale":{cell_scale}}}"#
        );
        std::fs::write(dir.join("pet.json"), manifest).unwrap();
        let (w, h) = (CELL_W * cell_scale, CELL_H * cell_scale);
        let mut img = RgbaImage::new(w * COLUMNS, h * rows);
        // 各コマの中央に不透明な点（縮めても残るか見る）
        for r in 0..rows {
            for c in 0..COLUMNS {
                for dy in 0..20 {
                    for dx in 0..20 {
                        img.put_pixel(
                            c * w + w / 2 - 10 + dx,
                            r * h + h / 2 - 10 + dy,
                            image::Rgba([10, 200, 10, 255]),
                        );
                    }
                }
            }
        }
        img.save(dir.join("sheet.png")).unwrap();
    }

    #[test]
    fn builtin_comes_first_and_first_root_wins() {
        let a = temp_root("a");
        let b = temp_root("b");
        write_pet(&a, "z-mame", "mame", "まめ", 1, 1);
        write_pet(&b, "mame-copy", "mame", "まめ（別）", 1, 1);
        write_pet(&b, "neko", "neko", "ねこ", 1, 1);
        std::fs::create_dir_all(b.join("broken")).unwrap();
        std::fs::write(b.join("broken").join("pet.json"), "{ not json").unwrap();
        std::fs::create_dir_all(b.join("empty")).unwrap();
        let pets = discover_in(&[a.clone(), b.clone(), PathBuf::from("Z:/does/not/exist")]);
        let ids: Vec<_> = pets
            .iter()
            .map(|p| (p.id.as_str(), p.name.as_str()))
            .collect();
        assert_eq!(
            ids,
            [
                ("placeholder", "仮キャラ"),
                ("mame", "まめ"),
                ("neko", "ねこ")
            ]
        );
        assert_eq!(pets[1].dir.as_deref(), Some(a.join("z-mame").as_path()));
        // 起動時に試す順: 選んだもの → フォルダのペット → 仮キャラ（重複しない）
        let ids = |o: Vec<&PetEntry>| o.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
        assert_eq!(
            ids(startup_order(&pets, None)),
            ["mame", "neko", "placeholder"]
        );
        assert_eq!(
            ids(startup_order(&pets, Some("neko"))),
            ["neko", "mame", "placeholder"]
        );
        assert_eq!(
            ids(startup_order(&pets, Some("placeholder"))),
            ["placeholder", "mame", "neko"]
        );
        assert_eq!(
            ids(startup_order(&pets, Some("gone"))),
            ["mame", "neko", "placeholder"]
        );
        assert_eq!(ids(startup_order(&discover_in(&[]), None)), ["placeholder"]);
    }

    #[test]
    fn wake_word_comes_from_pet_json() {
        let root = temp_root("wake");
        write_pet(&root, "mame", "mame", "まめ", 1, 1);
        write_pet(&root, "neko", "neko", "ねこ", 1, 1);
        let m = r#"{"id":"mame","displayName":"まめ","spritesheetPath":"sheet.png","wakeWord":" まめちゃん "}"#;
        std::fs::write(root.join("mame").join("pet.json"), m).unwrap();
        let pets = discover_in(std::slice::from_ref(&root));
        let words: Vec<_> = pets
            .iter()
            .map(|p| (p.id.as_str(), p.wake_word.as_deref()))
            .collect();
        assert_eq!(
            words,
            [
                ("placeholder", None),
                ("mame", Some("まめちゃん")),
                ("neko", None)
            ]
        );
        assert_eq!(
            entry_for_dir(&root.join("mame"))
                .unwrap()
                .wake_word
                .as_deref(),
            Some("まめちゃん")
        );
    }

    #[test]
    fn high_resolution_sheet_is_shrunk_to_what_is_needed() {
        let root = temp_root("shrink");
        write_pet(&root, "big", "big", "大", 2, 11);
        let entry = entry_for_dir(&root.join("big")).unwrap();
        // 表示倍率 1.1（大きさ 0.55 × 200%）なら 1 コマ 212×229 で足りる
        let need = needed_cell(1.1);
        assert_eq!(need, (212, 229));
        let s = load(&entry, need).unwrap();
        assert_eq!(
            (s.cell, s.source_cell, s.layout.rows()),
            ((212, 229), (384, 416), 11)
        );
        // 縮めてもコマの中央の点は残り、コマの端は透明のまま
        let c = s.cells.cell_image(0, 0);
        assert_eq!(c.dimensions(), (212, 229));
        let center = c.get_pixel(212 / 2, 229 / 2);
        assert!(center[3] > 200, "{center:?}");
        assert_eq!(c.get_pixel(212 - 1, 229 / 2)[3], 0);
        // 持つのは切り抜いた点のまわりだけ（コマ全体の 1 割未満）
        assert!(
            s.cells.bytes() < (212 * 229 * 4 * 88) / 10,
            "{}",
            s.cells.bytes()
        );
        // 持っていないコマ（範囲外）は透明
        assert!(s.cells.cell_image(10, 99).pixels().all(|p| p[3] == 0));
        // 要る大きさが元より大きければ、元のまま（引き伸ばさない）
        let s = load(&entry, needed_cell(3.0)).unwrap();
        assert_eq!(s.cell, (384, 416));
    }

    #[test]
    fn builtin_loads_and_bad_sheets_are_rejected() {
        let builtin = discover_in(&[]).remove(0);
        let s = load(&builtin, needed_cell(1.0)).unwrap();
        assert_eq!((s.cell, s.layout.rows()), ((CELL_W, CELL_H), 11));
        let root = temp_root("bad");
        write_pet(&root, "tiny", "tiny", "小", 1, 1);
        // コマの高さより低いシート（cellScale を大きく書き間違えた）
        let m = r#"{"id":"tiny","displayName":"小","spritesheetPath":"sheet.png","cellScale":4}"#;
        std::fs::write(root.join("tiny").join("pet.json"), m).unwrap();
        let entry = entry_for_dir(&root.join("tiny")).unwrap();
        assert!(load(&entry, needed_cell(1.0)).is_err());
    }

    /// 上限は動画のあるペットだけ 3/4。`full` ならどちらも元の大きさ（2026-10-27 画質の試し）
    #[test]
    fn max_cell_is_three_quarters_only_with_clips_unless_full() {
        assert_eq!(max_cell_with((384, 416), true, false), (288, 312));
        assert_eq!(max_cell_with((384, 416), false, false), (384, 416));
        assert_eq!(max_cell_with((384, 416), true, true), (384, 416));
        // 試験の切り替えが無い普段の経路は 3/4（環境変数を立てていないテストプロセスで）
        assert_eq!(max_cell((384, 416), true), (288, 312));
    }
}
