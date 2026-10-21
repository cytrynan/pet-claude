//! 動画クリップのコマを読み、描ける形で持つ（2026-10-21 K 指示・まめドラゴンの 13 本）。
//!
//! 13 本 361 コマを窓の大きさで全部展開すると、K の大きさ 105%（1 コマ 404×437）で 250MB を超え、
//! PRD N1（普段の専有 50MB）に収まらない。そこで、流すクリップだけを別スレッドで読み、窓の大きさに縮めて
//! 事前乗算済みの BGRA で、不透明な範囲だけを持つ。持つ量には上限を置き、使った順の古いものから捨てる。
//! 描くときはコピーするだけなので、ループを流し続けても CPU は小さい。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pet_core::clips::ClipSet;
use pet_core::pixels::{Cropped, compose, crop, render_bgra};

/// 持つクリップのコマの合計の上限。いま流しているクリップと、固定したクリップ（待機のループ）は超えても捨てない。
/// 2026-10-21 実測（K の大きさ 150%・100% の画面・1 コマ 288×312）: 待機のループ 9.6MB・歩き 8.5MB・つなぎ 3.4〜6MB。
/// 上限 20MB で固定なしだと散歩の一巡ごとに待機を読み直し、28MB にすると散歩中の専有が 49.4MB と PRD N1（50MB）
/// すれすれになった。上限 20MB で待機を固定すると、散歩中も待機と歩きとつなぎ 1 本で収まる。つまみ中（4.9MB）も固定する（下の `pinned`）
pub const BUDGET_BYTES: usize = 20 * 1024 * 1024;

/// ペットのフォルダの clips.json と、コマのある場所。
#[derive(Debug, Clone)]
pub struct ClipPack {
    pub set: ClipSet,
    /// `clips/` の親（ペットのフォルダ）
    pub dir: PathBuf,
}

impl ClipPack {
    /// `clips.json` を読み、全クリップのコマのファイルがそろっているか確かめる（欠けたまま流すと窓が空になる）。
    pub fn load(pet_dir: &Path, rel: &str) -> Result<ClipPack, String> {
        if !pet_core::sprite::is_inside_relative(rel) {
            return Err(format!(
                "clips はペットのフォルダの中の相対パスにする: {rel}"
            ));
        }
        let text = std::fs::read_to_string(pet_dir.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
        let set = ClipSet::from_json(&text)?;
        for c in &set.clips {
            for i in 0..c.frames {
                let p = frame_path(pet_dir, &c.name, i);
                if !p.is_file() {
                    return Err(format!("クリップ {} のコマが無い: {}", c.name, p.display()));
                }
            }
        }
        Ok(ClipPack {
            set,
            dir: pet_dir.to_path_buf(),
        })
    }
}

fn frame_path(pet_dir: &Path, name: &str, i: u32) -> PathBuf {
    pet_dir
        .join("clips")
        .join(name)
        .join(format!("{i:03}.webp"))
}

/// 1 本のクリップの、窓の大きさに縮めたコマ。
pub struct ClipFrames {
    pub size: (u32, u32),
    /// コマごとの (左上, 幅, 高さ, BGRA)。全部透明なコマは None
    frames: Vec<Option<Cropped>>,
}

impl ClipFrames {
    /// クリップのコマを全部読み、`size` に縮めて不透明な範囲で切り抜く（別スレッドで呼ぶ）。
    pub fn load(
        pack_dir: &Path,
        name: &str,
        frames: u32,
        size: (u32, u32),
    ) -> Result<ClipFrames, String> {
        let mut out = Vec::with_capacity(frames as usize);
        for i in 0..frames {
            let p = frame_path(pack_dir, name, i);
            let img = image::open(&p)
                .map_err(|e| format!("{}: {e}", p.display()))?
                .into_rgba8();
            let bgra = render_bgra(img, size.0, size.1);
            out.push(crop(&bgra, size));
        }
        Ok(ClipFrames { size, frames: out })
    }

    /// 持っている画素のバイト数。
    pub fn bytes(&self) -> usize {
        self.frames.iter().flatten().map(|f| f.4.len()).sum()
    }

    /// コマ `frame` を窓いっぱいの BGRA にする。`mirror` なら左右反転する。
    pub fn compose(&self, frame: u32, mirror: bool) -> Vec<u8> {
        compose(
            self.frames.get(frame as usize).and_then(Option::as_ref),
            self.size,
            mirror,
        )
    }
}

/// 読んだクリップを、使った順で上限まで持つ。
#[derive(Default)]
pub struct ClipCache {
    map: HashMap<usize, ClipFrames>,
    /// 使った順（後ろほど新しい）
    order: Vec<usize>,
    /// 捨てないクリップ。待機のループ（いちばん長く流す。捨てると散歩や反応のたびに読み直しになる）と、
    /// つまみ中のループ（つまんだ瞬間に出したい。読み込みを待つと 0.5 秒ほど直前の絵のままだった。
    /// 2026-10-21 K が実機で触って分かった）
    pub pinned: Vec<usize>,
}

impl ClipCache {
    /// `pinned` を捨てない入れ物。
    pub fn with_pinned(pinned: Vec<usize>) -> Self {
        Self {
            pinned,
            ..Self::default()
        }
    }

    pub fn get(&mut self, index: usize) -> Option<&ClipFrames> {
        if self.map.contains_key(&index) {
            self.order.retain(|&i| i != index);
            self.order.push(index);
        }
        self.map.get(&index)
    }

    pub fn contains(&self, index: usize) -> bool {
        self.map.contains_key(&index)
    }

    /// 入れて、上限を超えた分を古い順に捨てる。`keep`（いま流しているクリップ）は捨てない。
    pub fn insert(&mut self, index: usize, frames: ClipFrames, keep: Option<usize>) {
        self.map.insert(index, frames);
        self.order.retain(|&i| i != index);
        self.order.push(index);
        while self.bytes() > BUDGET_BYTES {
            let Some(pos) = self
                .order
                .iter()
                .position(|&i| Some(i) != keep && !self.pinned.contains(&i) && i != index)
            else {
                break;
            };
            let old = self.order.remove(pos);
            self.map.remove(&old);
        }
    }

    /// これから `need` バイトを読み込むので、上限に収まるまで古い順に捨てておく（`keep` は捨てない）。
    /// 読み込んでから捨てると、読み込みの間だけ上限を 1 本分超える。
    pub fn make_room(&mut self, need: usize, keep: Option<usize>) {
        while self.bytes() + need > BUDGET_BYTES {
            let Some(pos) = self
                .order
                .iter()
                .position(|&i| Some(i) != keep && !self.pinned.contains(&i))
            else {
                break;
            };
            let old = self.order.remove(pos);
            self.map.remove(&old);
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }

    pub fn bytes(&self) -> usize {
        self.map.values().map(ClipFrames::bytes).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(size: (u32, u32), n: usize, bytes_each: usize) -> ClipFrames {
        ClipFrames {
            size,
            frames: (0..n)
                .map(|_| Some((0, 0, 1, 1, vec![0; bytes_each])))
                .collect(),
        }
    }

    #[test]
    fn pinned_clip_is_never_evicted() {
        let mut c = ClipCache {
            pinned: vec![0],
            ..ClipCache::default()
        };
        let half = BUDGET_BYTES / 2 - 1;
        c.insert(0, frames((1, 1), 1, half), None);
        c.insert(1, frames((1, 1), 1, half), None);
        c.insert(2, frames((1, 1), 1, half), Some(2));
        assert!(c.contains(0) && !c.contains(1) && c.contains(2));
        c.make_room(BUDGET_BYTES, None);
        assert!(c.contains(0) && !c.contains(2));
    }

    #[test]
    fn make_room_frees_space_before_loading() {
        let mut c = ClipCache::default();
        let third = BUDGET_BYTES / 3;
        c.insert(0, frames((1, 1), 1, third), None);
        c.insert(1, frames((1, 1), 1, third), None);
        // 0 を流している。半分の大きさを読む前に、古い順で 0 以外（1）を捨てる
        c.make_room(BUDGET_BYTES / 2, Some(0));
        assert!(c.contains(0) && !c.contains(1));
        // 流しているものしか無ければ、上限を超えても捨てない
        c.make_room(BUDGET_BYTES, Some(0));
        assert!(c.contains(0));
    }

    #[test]
    fn cache_evicts_oldest_but_keeps_the_playing_clip() {
        let mut c = ClipCache::default();
        let half = BUDGET_BYTES / 2 - 1;
        c.insert(0, frames((1, 1), 1, half), None);
        c.insert(1, frames((1, 1), 1, half), None);
        assert!(c.contains(0) && c.contains(1));
        // 0 を使い直すと、次に捨てられるのは 1
        assert!(c.get(0).is_some());
        c.insert(2, frames((1, 1), 1, half), Some(0));
        assert!(c.contains(0) && !c.contains(1) && c.contains(2));
        // いま流しているクリップは、上限を超えても捨てない
        c.insert(3, frames((1, 1), 1, BUDGET_BYTES), Some(0));
        assert!(c.contains(0) && c.contains(3) && !c.contains(2));
    }

    #[test]
    fn mame_dragon_clips_are_complete_and_load() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/pets/mame-dragon");
        let pack = ClipPack::load(&dir, "clips.json").expect("まめドラゴンのクリップ");
        assert_eq!(pack.set.clips.len(), 13);
        let idle = pack.set.clips.iter().find(|c| c.name == "idle").unwrap();
        let f = ClipFrames::load(&pack.dir, "idle", idle.frames, (202, 219)).unwrap();
        // 透明な背景が抜けていて、切り抜いた範囲は窓より小さい
        assert!(
            f.bytes() < (202 * 219 * 4 * idle.frames as usize),
            "{}",
            f.bytes()
        );
        let px = f.compose(0, false);
        assert_eq!(&px[0..4], &[0, 0, 0, 0], "左上の隅は透明");
    }
}
