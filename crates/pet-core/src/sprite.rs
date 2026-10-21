//! スプライトシートの規約。Codex ペット互換（1 コマ 192×208・8 列・行ごとに 1 ポーズ）。
//!
//! 行 0〜8 は Codex 互換、行 9（睡眠）と行 10（つままれ）は pet-claude 独自。
//! 独自行が無いシート（Codex の 9 行素材）では代替行で動く。

use serde::Deserialize;

pub const CELL_W: u32 = 192;
pub const CELL_H: u32 = 208;
pub const COLUMNS: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pose {
    Idle,
    RunRight,
    RunLeft,
    Waving,
    Jumping,
    Failed,
    Waiting,
    Running,
    Review,
    Sleep,
    Grabbed,
}

/// ポーズの再生仕様。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clip {
    pub row: u32,
    pub frames: u32,
    /// 1 周にかかる時間。
    pub cycle_ms: u32,
    /// 何周で止めるか（None なら繰り返し続ける）。
    pub loops: Option<u32>,
}

impl Pose {
    /// シートに専用行があるときの再生仕様（Codex の値に合わせる）。
    pub const fn native_clip(self) -> Clip {
        let (row, frames, cycle_ms, loops) = match self {
            Self::Idle => (0, 6, 5500, None),
            Self::RunRight => (1, 8, 1060, None),
            Self::RunLeft => (2, 8, 1060, None),
            Self::Waving => (3, 4, 700, Some(2)),
            Self::Jumping => (4, 5, 840, Some(2)),
            Self::Failed => (5, 8, 1220, Some(2)),
            Self::Waiting => (6, 6, 1010, None),
            Self::Running => (7, 6, 820, None),
            Self::Review => (8, 6, 1030, None),
            Self::Sleep => (9, 6, 4000, None),
            Self::Grabbed => (10, 4, 600, None),
        };
        Clip {
            row,
            frames,
            cycle_ms,
            loops,
        }
    }

    /// `rows` 行がすべて既定のコマ数で埋まったシートで再生する仕様（`clip_in` の簡易版）。
    pub fn clip_for(self, rows: u32) -> Clip {
        self.clip_in(&SheetLayout::full(rows))
    }

    /// 実際のシート（`SheetLayout`）で再生する仕様。
    ///
    /// - 行があれば、その行で数えたコマ数で回す。1 コマの長さは Codex の値のまま
    ///   （既定のコマ数と同じなら Codex の 1 周の時間そのもの）
    /// - 行が無い・中身が空なら代わりの行へ: 睡眠は待機を半分の速さで、つままれは入力待ち、ほかは待機。
    ///   待機も空なら、中身のある最初の行
    ///
    /// 空の行を再生すると窓が全部透明になり、クリックも右クリックも背後へ抜けてメニューから戻せない
    /// （2026-10-21 embed-pet の解析で推された取り込み）。
    pub fn clip_in(self, layout: &SheetLayout) -> Clip {
        let native = self.native_clip();
        if let Some(clip) = layout.clip_on_row(native) {
            return clip;
        }
        match self {
            Self::Sleep => {
                let idle = Self::Idle.clip_in(layout);
                Clip {
                    cycle_ms: idle.cycle_ms * 2,
                    ..idle
                }
            }
            Self::Grabbed => Self::Waiting.clip_in(layout),
            Self::Idle => layout.first_filled_row(native),
            _ => Self::Idle.clip_in(layout),
        }
    }
}

/// シートの行数の上限（Codex 互換 9 行＋睡眠・つままれ）。
pub const MAX_ROWS: usize = 11;

/// 実際のシートで、行ごとに中身のあるコマが何枚あるか（0 は「行が無い」）。
/// 読み込み時と切り替え時に `SheetLayout::measure` で測る（embed-pet の「コマ数はシートのアルファから
/// 数える」と同じ考え方。シートに動きの情報を持たせずに、Codex の 9 行・独自の 11 行・行が欠けたシートを扱う）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SheetLayout {
    pub frames: [u32; MAX_ROWS],
}

/// この値以下のアルファしかないコマは「空」とみなす（縁のかすかなにじみを中身と数えない）。
pub const EMPTY_ALPHA: u8 = 8;

impl SheetLayout {
    /// `rows` 行がすべて既定のコマ数で埋まっているとみなす（素材を測らないとき・テスト用）。
    pub fn full(rows: u32) -> Self {
        let mut frames = [0; MAX_ROWS];
        for (row, f) in frames.iter_mut().enumerate().take(rows as usize) {
            *f = ALL_POSES
                .iter()
                .find(|p| p.native_clip().row as usize == row)
                .map_or(COLUMNS, |p| p.native_clip().frames);
        }
        Self { frames }
    }

    /// シートを測る。各行を左から見て中身のあるコマを数え、最初の空のコマで止める（途中に穴があれば
    /// 穴の手前まで）。`cell` は 1 コマの画素数（`PetManifest::cell_size`）。
    pub fn measure(sheet: &image::RgbaImage, cell: (u32, u32)) -> Self {
        let mut frames = [0; MAX_ROWS];
        let rows = (sheet.height() / cell.1.max(1)).min(MAX_ROWS as u32);
        let cols = (sheet.width() / cell.0.max(1)).min(COLUMNS);
        for row in 0..rows {
            let mut n = 0;
            for col in 0..cols {
                if !cell_has_content(sheet, col * cell.0, row * cell.1, cell) {
                    break;
                }
                n += 1;
            }
            frames[row as usize] = n;
        }
        Self { frames }
    }

    /// 中身のある行が 1 つも無い。
    pub fn is_empty(&self) -> bool {
        self.frames.iter().all(|&f| f == 0)
    }

    /// 中身のある最後の行の次（従来の「行数」）。
    pub fn rows(&self) -> u32 {
        self.frames
            .iter()
            .rposition(|&f| f > 0)
            .map_or(0, |i| i as u32 + 1)
    }

    fn clip_on_row(&self, native: Clip) -> Option<Clip> {
        let n = *self.frames.get(native.row as usize)?;
        (n > 0).then(|| Clip {
            frames: n,
            cycle_ms: if n == native.frames {
                native.cycle_ms
            } else {
                native.cycle_ms / native.frames * n
            },
            ..native
        })
    }

    /// 待機も空のとき: 中身のある最初の行を、待機の 1 コマの長さで回す。全部空なら行 0 の 1 コマ。
    fn first_filled_row(&self, idle: Clip) -> Clip {
        let frame_ms = idle.cycle_ms / idle.frames;
        match self.frames.iter().position(|&f| f > 0) {
            Some(row) => Clip {
                row: row as u32,
                frames: self.frames[row],
                cycle_ms: frame_ms * self.frames[row],
                ..idle
            },
            None => Clip {
                frames: 1,
                cycle_ms: frame_ms,
                ..idle
            },
        }
    }
}

const ALL_POSES: [Pose; MAX_ROWS] = [
    Pose::Idle,
    Pose::RunRight,
    Pose::RunLeft,
    Pose::Waving,
    Pose::Jumping,
    Pose::Failed,
    Pose::Waiting,
    Pose::Running,
    Pose::Review,
    Pose::Sleep,
    Pose::Grabbed,
];

fn cell_has_content(sheet: &image::RgbaImage, x0: u32, y0: u32, cell: (u32, u32)) -> bool {
    (y0..y0 + cell.1).any(|y| (x0..x0 + cell.0).any(|x| sheet.get_pixel(x, y)[3] > EMPTY_ALPHA))
}

/// 再生中のアニメーション。時刻を渡すとコマ番号を返す純粋な時計。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Playback {
    pub clip: Clip,
    pub started_ms: u64,
}

impl Playback {
    pub const fn frame_ms(&self) -> u32 {
        let f = self.clip.cycle_ms / self.clip.frames;
        if f == 0 { 1 } else { f }
    }

    /// (コマ番号, 再生し終えたか)。
    pub fn frame_at(&self, now_ms: u64) -> (u32, bool) {
        let elapsed = now_ms.saturating_sub(self.started_ms);
        let step = elapsed / u64::from(self.frame_ms());
        let frames = u64::from(self.clip.frames);
        match self.clip.loops {
            Some(n) if step >= frames * u64::from(n) => (self.clip.frames - 1, true),
            _ => ((step % frames) as u32, false),
        }
    }
}

/// シート上のコマの位置（ピクセル）。
pub const fn cell_origin(row: u32, frame: u32) -> (u32, u32) {
    (frame * CELL_W, row * CELL_H)
}

/// シートの高さから行数を求める（端数は切り捨て）。
pub const fn rows_in(sheet_height: u32) -> u32 {
    sheet_height / CELL_H
}

/// `assets/pets/<name>/pet.json`（Codex 互換の項目＋pet-claude 独自の `cellScale`）。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PetManifest {
    pub id: String,
    pub display_name: String,
    pub spritesheet_path: String,
    /// 1 コマの画素を `CELL_W`×`CELL_H` の何倍で描いたか（無ければ 1＝Codex 互換）。
    /// 表示倍率 200% の画面で引き伸ばさずに済むよう、高精細の素材は 2 で作る
    #[serde(default)]
    pub cell_scale: Option<u32>,
}

/// 受け付ける `cellScale` の上限（4 倍なら 768×832 のコマ。それ以上は読み込みが重すぎる）。
pub const MAX_CELL_SCALE: u32 = 4;

impl PetManifest {
    /// 1 コマの画素数 (幅, 高さ)。`cellScale` は 1〜`MAX_CELL_SCALE` に丸める。
    pub fn cell_size(&self) -> (u32, u32) {
        let s = self.cell_scale.unwrap_or(1).clamp(1, MAX_CELL_SCALE);
        (CELL_W * s, CELL_H * s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_sheet_has_nine_rows_and_extended_has_eleven() {
        assert_eq!(rows_in(1872), 9);
        assert_eq!(rows_in(2288), 11);
    }

    #[test]
    fn extended_sheet_uses_its_own_sleep_and_grabbed_rows() {
        assert_eq!(Pose::Sleep.clip_for(11).row, 9);
        assert_eq!(Pose::Grabbed.clip_for(11).row, 10);
    }

    #[test]
    fn codex_sheet_falls_back_for_sleep_and_grabbed() {
        let sleep = Pose::Sleep.clip_for(9);
        assert_eq!((sleep.row, sleep.cycle_ms), (0, 11000));
        assert_eq!(Pose::Grabbed.clip_for(9).row, 6);
    }

    /// `rows` 行のシートを作り、`filled[row]` 枚のコマ（左から）に 1 画素だけ不透明な点を打つ。
    fn sheet(cell: (u32, u32), rows: u32, filled: &[(usize, &[u32])]) -> image::RgbaImage {
        let mut img = image::RgbaImage::new(cell.0 * COLUMNS, cell.1 * rows);
        for (row, cols) in filled {
            for &col in *cols {
                img.put_pixel(
                    col * cell.0 + cell.0 / 2,
                    *row as u32 * cell.1 + cell.1 - 3,
                    image::Rgba([1, 2, 3, 255]),
                );
            }
        }
        img
    }

    #[test]
    fn measured_codex_sheet_plays_like_before() {
        // ① 9 行の Codex シート（既定のコマ数で埋まっている）は、いまと同じ再生になる
        let cell = (CELL_W, CELL_H);
        let rows: Vec<(usize, Vec<u32>)> = ALL_POSES[..9]
            .iter()
            .map(|p| {
                (
                    p.native_clip().row as usize,
                    (0..p.native_clip().frames).collect(),
                )
            })
            .collect();
        let filled: Vec<(usize, &[u32])> = rows.iter().map(|(r, c)| (*r, c.as_slice())).collect();
        let layout = SheetLayout::measure(&sheet(cell, 9, &filled), cell);
        assert_eq!(layout, SheetLayout::full(9));
        for p in ALL_POSES {
            assert_eq!(p.clip_in(&layout), p.clip_for(9), "{p:?}");
        }
        assert_eq!(layout.rows(), 9);
    }

    #[test]
    fn empty_row_falls_back() {
        // ② 11 行あるが、手を振る（3）と見直し（8）が空のシート（静止画から組んだ素材など）
        let cell = (CELL_W, CELL_H);
        let all: Vec<u32> = (0..8).collect();
        let filled: Vec<(usize, &[u32])> = (0..11)
            .filter(|r| *r != 3 && *r != 8)
            .map(|r| (r, &all[..]))
            .collect();
        let layout = SheetLayout::measure(&sheet(cell, 11, &filled), cell);
        assert_eq!(layout.frames[3], 0);
        assert_eq!(Pose::Waving.clip_in(&layout).row, 0, "空の行は待機へ");
        assert_eq!(Pose::Review.clip_in(&layout).row, 0);
        assert_eq!(
            Pose::Sleep.clip_in(&layout).row,
            9,
            "中身のある行はそのまま"
        );
        // 睡眠もつままれも空なら、いまの代わりの連鎖（待機を半分の速さで・入力待ち）
        let filled: Vec<(usize, &[u32])> = (0..9).map(|r| (r, &all[..])).collect();
        let layout = SheetLayout::measure(&sheet(cell, 11, &filled), cell);
        let sleep = Pose::Sleep.clip_in(&layout);
        assert_eq!(
            (sleep.row, sleep.cycle_ms),
            (0, Pose::Idle.native_clip().cycle_ms / 6 * 8 * 2)
        );
        assert_eq!(Pose::Grabbed.clip_in(&layout).row, 6);
        // 待機まで空なら、中身のある最初の行
        let layout = SheetLayout::measure(&sheet(cell, 11, &[(4, &all[..2])]), cell);
        assert_eq!(
            (
                Pose::Idle.clip_in(&layout).row,
                Pose::Running.clip_in(&layout).row
            ),
            (4, 4)
        );
        assert!(SheetLayout::measure(&sheet(cell, 11, &[]), cell).is_empty());
    }

    #[test]
    fn single_frame_row_is_a_still_and_holes_stop_the_count() {
        let cell = (CELL_W, CELL_H);
        // ③ 1 コマだけの行は静止画（1 コマの長さは Codex の値のまま）
        // ④ 途中に穴がある行は穴の手前まで
        let layout =
            SheetLayout::measure(&sheet(cell, 11, &[(0, &[0]), (7, &[0, 1, 2, 4, 5])]), cell);
        let idle = Pose::Idle.clip_in(&layout);
        assert_eq!((idle.frames, idle.cycle_ms), (1, 5500 / 6));
        let running = Pose::Running.clip_in(&layout);
        assert_eq!((running.frames, running.cycle_ms), (3, 820 / 6 * 3));
    }

    #[test]
    fn measure_uses_the_manifest_cell_size() {
        // ⑤ cellScale 2 のシートは 384×416 のコマで数える（192×208 で数えると 2 倍の列に見える）
        let cell = (CELL_W * 2, CELL_H * 2);
        let layout =
            SheetLayout::measure(&sheet(cell, 11, &[(0, &[0, 1, 2]), (10, &[0, 1])]), cell);
        assert_eq!(
            (layout.frames[0], layout.frames[10], layout.rows()),
            (3, 2, 11)
        );
        // かすかなにじみ（アルファ EMPTY_ALPHA 以下）は中身と数えない
        let mut faint = image::RgbaImage::new(CELL_W * COLUMNS, CELL_H);
        faint.put_pixel(5, 5, image::Rgba([0, 0, 0, EMPTY_ALPHA]));
        assert!(SheetLayout::measure(&faint, (CELL_W, CELL_H)).is_empty());
    }

    #[test]
    fn tiny_sheet_falls_back_to_idle_row() {
        assert_eq!(Pose::Review.clip_for(1).row, 0);
        assert_eq!(Pose::Grabbed.clip_for(1).row, 0);
    }

    #[test]
    fn looping_clip_wraps_around() {
        let p = Playback {
            clip: Pose::Running.native_clip(),
            started_ms: 1000,
        };
        let f = u64::from(p.frame_ms());
        assert_eq!(p.frame_at(1000), (0, false));
        assert_eq!(p.frame_at(1000 + f * 5), (5, false));
        assert_eq!(p.frame_at(1000 + f * 6), (0, false));
    }

    #[test]
    fn finite_clip_stops_on_last_frame() {
        let p = Playback {
            clip: Pose::Waving.native_clip(),
            started_ms: 0,
        };
        let f = u64::from(p.frame_ms());
        assert_eq!(p.frame_at(f * 7), (3, false));
        assert_eq!(p.frame_at(f * 8), (3, true));
        assert_eq!(p.frame_at(f * 100), (3, true));
    }

    #[test]
    fn clock_before_start_is_first_frame() {
        let p = Playback {
            clip: Pose::Idle.native_clip(),
            started_ms: 500,
        };
        assert_eq!(p.frame_at(0), (0, false));
    }

    #[test]
    fn cell_origin_matches_codex_grid() {
        assert_eq!(cell_origin(0, 0), (0, 0));
        assert_eq!(cell_origin(8, 7), (1344, 1664));
    }

    #[test]
    fn manifest_reads_codex_fields() {
        let m: PetManifest = serde_json::from_str(
            r#"{"id":"nia","displayName":"Nia","spritesheetPath":"spritesheet.webp","extra":1}"#,
        )
        .unwrap();
        assert_eq!(m.spritesheet_path, "spritesheet.webp");
        // cellScale が無ければ Codex 互換の大きさ
        assert_eq!(m.cell_size(), (CELL_W, CELL_H));
    }

    #[test]
    fn manifest_cell_scale_is_clamped() {
        let with = |s: &str| -> PetManifest {
            serde_json::from_str(&format!(
                r#"{{"id":"a","displayName":"A","spritesheetPath":"s.webp","cellScale":{s}}}"#
            ))
            .unwrap()
        };
        assert_eq!(with("2").cell_size(), (CELL_W * 2, CELL_H * 2));
        assert_eq!(with("0").cell_size(), (CELL_W, CELL_H));
        assert_eq!(
            with("99").cell_size(),
            (CELL_W * MAX_CELL_SCALE, CELL_H * MAX_CELL_SCALE)
        );
    }
}
