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

    /// `rows` 行しかないシートで実際に再生する仕様。
    pub fn clip_for(self, rows: u32) -> Clip {
        let native = self.native_clip();
        if native.row < rows {
            return native;
        }
        match self {
            // 睡眠が無ければ待機を半分の速さで
            Self::Sleep => {
                let idle = Self::Idle.native_clip();
                Clip {
                    cycle_ms: idle.cycle_ms * 2,
                    ..idle
                }
            }
            Self::Grabbed => Self::Waiting.clip_for(rows),
            _ => Self::Idle.native_clip(),
        }
    }
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

/// `assets/pets/<name>/pet.json`（Codex 互換の項目だけ読む）。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PetManifest {
    pub id: String,
    pub display_name: String,
    pub spritesheet_path: String,
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
    }
}
