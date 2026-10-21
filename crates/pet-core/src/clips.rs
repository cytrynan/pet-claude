//! 動画クリップの再生計画。いまの見た目（姿勢と向き）から、行動が求めるポーズの姿勢まで、
//! つなぎの動画をどの順に流すかを決めて、時刻からいま描くコマを返す純粋な状態機械。
//!
//! 姿勢をそのまま切り替えると大きさや向きが急に変わるので、姿勢の間を動画でつなぐ
//! （2026-10-21 K 指示・まめドラゴンの 13 本）。OS には触らない。
//! 仕様の正本は `.steering/20261021-clip-playback/requirements.md`「計画の規則」と design.md §1・§4。

use std::collections::VecDeque;

use serde::Deserialize;

use crate::sprite::Pose;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Facing {
    Left,
    Right,
    Front,
}

impl Facing {
    /// 左右反転した向き（Front は Front のまま）。
    pub const fn flip(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::Front => Self::Front,
        }
    }
}

/// 動画の始点・終点になる姿勢。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Still {
    Idle,
    Walk,
    Sleep,
    Dragged,
    Attention,
    Working,
}

/// 見た目の状態＝(姿勢, 向き)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VState {
    pub still: Still,
    pub facing: Facing,
}

impl VState {
    const fn flip(self) -> Self {
        Self {
            still: self.still,
            facing: self.facing.flip(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipKind {
    /// from＝to。ポーズの間ずっと繰り返す
    Loop,
    /// from≠to。1 回流して to の姿勢になる
    Transition,
    /// from＝to だが 1 回だけ流す（喜ぶ・なでられ）
    Once,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipDef {
    pub name: String,
    pub kind: ClipKind,
    pub from: VState,
    pub to: VState,
    pub frames: u32,
}

impl ClipDef {
    /// 反転して流したときの始点と終点。
    fn ends(&self, mirror: bool) -> (VState, VState) {
        if mirror {
            (self.from.flip(), self.to.flip())
        } else {
            (self.from, self.to)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipSet {
    pub fps: u32,
    pub clips: Vec<ClipDef>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSet {
    fps: u32,
    clips: Vec<RawClip>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawClip {
    name: String,
    kind: String,
    from: (String, String),
    to: (String, String),
    frames: u32,
}

fn parse_state((still, facing): &(String, String)) -> Result<VState, String> {
    let still = match still.as_str() {
        "idle" => Still::Idle,
        "walk" => Still::Walk,
        "sleep" => Still::Sleep,
        "dragged" => Still::Dragged,
        "attention" => Still::Attention,
        "working" => Still::Working,
        s => return Err(format!("姿勢 {s} は知らない")),
    };
    let facing = match facing.as_str() {
        "left" => Facing::Left,
        "right" => Facing::Right,
        "front" => Facing::Front,
        f => return Err(format!("向き {f} は知らない")),
    };
    // つままれと呼びかけは正面の絵しか無い。向きを持たせると反転の計画が狂う
    if matches!(still, Still::Dragged | Still::Attention) != (facing == Facing::Front) {
        return Err(format!(
            "{still:?} の向きが {facing:?}（Dragged・Attention だけが Front）"
        ));
    }
    Ok(VState { still, facing })
}

impl ClipSet {
    /// clips.json を読む。値が壊れていたら Err（読めないクリップを流すと窓が空になるため、読み込みで止める）。
    pub fn from_json(text: &str) -> Result<ClipSet, String> {
        let raw: RawSet = serde_json::from_str(text).map_err(|e| format!("clips.json: {e}"))?;
        if raw.fps == 0 {
            return Err("fps が 0".into());
        }
        let mut clips: Vec<ClipDef> = Vec::with_capacity(raw.clips.len());
        for c in raw.clips {
            if c.frames == 0 {
                return Err(format!("{}: frames が 0", c.name));
            }
            if clips.iter().any(|d| d.name == c.name) {
                return Err(format!("{}: 名前が重複", c.name));
            }
            let kind = match c.kind.as_str() {
                "loop" => ClipKind::Loop,
                "transition" => ClipKind::Transition,
                "once" => ClipKind::Once,
                k => return Err(format!("{}: 種類 {k} は知らない", c.name)),
            };
            let from = parse_state(&c.from)?;
            let to = parse_state(&c.to)?;
            if (kind == ClipKind::Transition) == (from == to) {
                return Err(format!("{}: {kind:?} なのに from と to が合わない", c.name));
            }
            clips.push(ClipDef {
                name: c.name,
                kind,
                from,
                to,
                frames: c.frames,
            });
        }
        Ok(ClipSet {
            fps: raw.fps,
            clips,
        })
    }

    /// 1 回流す長さ（ms・切り捨て。design.md §4-1）。
    fn duration_ms(&self, index: usize) -> u64 {
        u64::from(self.clips[index].frames) * 1000 / u64::from(self.fps)
    }

    fn loop_for(&self, still: Still) -> Option<usize> {
        self.clips
            .iter()
            .position(|c| c.kind == ClipKind::Loop && c.from.still == still)
    }

    fn once_named(&self, name: &str) -> Option<usize> {
        self.clips
            .iter()
            .position(|c| c.kind == ClipKind::Once && c.name == name)
    }
}

/// いま描くもの。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Show {
    /// `clips[index]` のコマ `frame` を描く。`mirror` なら左右反転して描く
    Clip {
        index: usize,
        frame: u32,
        mirror: bool,
    },
    /// クリップを使わずシートの行で描く（Failed、またはつなぎ無しで来たループの無い姿勢）
    Sheet(Pose),
}

/// ポーズの目標の姿勢（requirements.md「ポーズ → 目標」）。Failed はクリップを使わないので None。
pub const fn still_for(pose: Pose) -> Option<Still> {
    match pose {
        Pose::RunRight | Pose::RunLeft => Some(Still::Walk),
        Pose::Sleep => Some(Still::Sleep),
        Pose::Grabbed => Some(Still::Dragged),
        Pose::Waiting | Pose::Waving => Some(Still::Attention),
        Pose::Running | Pose::Review => Some(Still::Working),
        Pose::Idle | Pose::Jumping | Pose::Petted => Some(Still::Idle),
        Pose::Failed => None,
    }
}

/// クリップがあってもシートの行で描くことがあるポーズ。Failed と、ループの無い姿勢（`Player` が
/// つなぎ無しでそこへ来たら `Show::Sheet` を返す）。アプリはこの行だけシートから持つ（メモリの節約）。
pub fn sheet_poses(set: &ClipSet) -> Vec<Pose> {
    const ALL: [Pose; 12] = [
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
        Pose::Petted,
    ];
    ALL.into_iter()
        .filter(|&p| still_for(p).is_none_or(|s| set.loop_for(s).is_none()))
        .collect()
}

/// 目標の見た目。向きが None なら「どちらでもよい」。
#[derive(Debug, Clone, Copy)]
struct Target {
    still: Still,
    facing: Option<Facing>,
}

impl Target {
    fn matches(self, s: VState) -> bool {
        s.still == self.still && self.facing.is_none_or(|f| f == s.facing)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cur {
    /// ループを `start` から流している
    Loop {
        index: usize,
        mirror: bool,
        start: u64,
    },
    /// つなぎ・単発を `start` から流している（終わったら `at` を終点にして次へ）
    Play {
        index: usize,
        mirror: bool,
        start: u64,
    },
    /// ループの無い姿勢に、つなぎの最後のコマで止まっている
    Hold { index: usize, mirror: bool },
    /// シートの行で描いている
    Sheet(Pose),
}

pub struct Player {
    set: ClipSet,
    /// 行動が求めている最新のポーズ
    desired: Pose,
    /// 単発（喜ぶ・なでられ）をまだ流していない。そのポーズに入るたびに立てる
    once_owed: bool,
    /// いまいる見た目の状態（流しているつなぎの始点。終わったら終点に進める）
    at: VState,
    cur: Cur,
    /// これから流すつなぎ・単発
    queue: VecDeque<(usize, bool)>,
    /// 直前に流し終えたつなぎ（ループの無い姿勢でその最後のコマを止めて出すため）
    arrived_by: Option<(usize, bool)>,
    /// 直前の `tick` で歩きのループを流していたか（`may_move` が返す値。design.md §4-2）
    moving: bool,
}

const MAX_PATH: usize = 6;

impl Player {
    /// 最初は、`pose` の目標の姿勢につなぎ無しで置く（向きは Left。向きの決まるポーズはその向き）。
    pub fn new(set: ClipSet, pose: Pose, now_ms: u64) -> Player {
        let mut p = Player {
            set,
            desired: pose,
            once_owed: false,
            at: VState {
                still: Still::Idle,
                facing: Facing::Left,
            },
            cur: Cur::Sheet(pose),
            queue: VecDeque::new(),
            arrived_by: None,
            moving: false,
        };
        if pose == Pose::Failed {
            // Failed はシートで表示する（見た目の状態は既定の (Idle, Left) のまま）。検証係の指摘 2026-10-21
            return p;
        }
        let target = p.target_for(pose);
        p.at = VState {
            still: target.still,
            facing: target.facing.unwrap_or(Facing::Left),
        };
        p.settle(now_ms);
        p
    }

    pub fn clips(&self) -> &ClipSet {
        &self.set
    }

    /// 次に流す予定のつなぎ・単発（アプリが先に読み込んでおくため）。
    pub fn next_clip(&self) -> Option<usize> {
        self.queue.front().map(|&(i, _)| i)
    }

    /// 行動が求めるポーズを伝える。同じポーズなら何もしない。
    pub fn set_pose(&mut self, pose: Pose, now_ms: u64) {
        if pose == self.desired {
            return;
        }
        self.desired = pose;
        self.once_owed = matches!(pose, Pose::Jumping | Pose::Petted);
        if pose == Pose::Grabbed {
            // 規則 6: つまみ上げはすぐに見せる（つなぎの途中でも切る）
            self.queue.clear();
            self.arrived_by = None;
            self.at = VState {
                still: Still::Dragged,
                facing: Facing::Front,
            };
            self.settle(now_ms);
            return;
        }
        // 規則 7: つなぎ・単発の最中は最後まで流してから、終点から計画し直す（tick が行う）。
        // 積んでいた残りは前のポーズ向けなので捨てる（捨てないと、流し終えたあと古い計画を続ける）
        if matches!(self.cur, Cur::Play { .. }) {
            self.queue.clear();
        } else {
            self.replan(now_ms);
        }
    }

    /// いま描くものを返す。
    pub fn tick(&mut self, now_ms: u64) -> Show {
        let show = self.advance(now_ms);
        self.moving = matches!(self.cur, Cur::Loop { index, .. } if self.set.clips[index].from.still == Still::Walk);
        show
    }

    fn advance(&mut self, now_ms: u64) -> Show {
        // 1 回の tick で複数のクリップの終わりを越えたら、越えた分だけ順に進める（終わった時刻から次を始める）
        while let Cur::Play {
            index,
            mirror,
            start,
        } = self.cur
        {
            let end = start + self.set.duration_ms(index);
            if now_ms < end {
                let frame = ((now_ms - start) * u64::from(self.set.fps) / 1000) as u32;
                return Show::Clip {
                    index,
                    frame: frame.min(self.set.clips[index].frames - 1),
                    mirror,
                };
            }
            self.at = self.set.clips[index].ends(mirror).1;
            self.arrived_by =
                (self.set.clips[index].kind == ClipKind::Transition).then_some((index, mirror));
            if !self.start_next(end) {
                self.replan(end);
            }
        }
        match self.cur {
            Cur::Loop {
                index,
                mirror,
                start,
            } => {
                let step = now_ms.saturating_sub(start) * u64::from(self.set.fps) / 1000;
                Show::Clip {
                    index,
                    frame: (step % u64::from(self.set.clips[index].frames)) as u32,
                    mirror,
                }
            }
            Cur::Hold { index, mirror } => Show::Clip {
                index,
                frame: self.set.clips[index].frames - 1,
                mirror,
            },
            Cur::Sheet(pose) => Show::Sheet(pose),
            Cur::Play { .. } => unreachable!("上の while で Play は抜けている"),
        }
    }

    /// 窓を横に動かしてよいか（直前の `tick` で Walk のループを流していたときだけ。`set_pose` では変わらない）。
    pub fn may_move(&self) -> bool {
        self.moving
    }

    /// ポーズの目標（requirements.md「ポーズ → 目標」）。Failed と単発は呼び出し側で扱う。
    fn target_for(&self, pose: Pose) -> Target {
        let keep = match self.at.facing {
            Facing::Front => None,
            f => Some(f),
        };
        let still = still_for(pose).unwrap_or(Still::Idle);
        let facing = match pose {
            Pose::RunRight => Some(Facing::Right),
            Pose::RunLeft => Some(Facing::Left),
            Pose::Grabbed | Pose::Waiting | Pose::Waving => Some(Facing::Front),
            _ => keep,
        };
        Target { still, facing }
    }

    /// いまの `at` から最新のポーズまでの計画を立て、`now` から流し始める。
    fn replan(&mut self, now: u64) {
        self.queue.clear();
        let pose = self.desired;
        if pose == Pose::Failed {
            // Failed はシートで表示し、見た目の状態は変えない
            self.cur = Cur::Sheet(Pose::Failed);
            return;
        }
        let target = self.target_for(pose);
        let once = match pose {
            Pose::Jumping if self.once_owed => self.set.once_named("happy"),
            Pose::Petted if self.once_owed => self.set.once_named("petted"),
            _ => None,
        };
        match self.path(self.at, target) {
            Some(steps) => self.queue.extend(steps),
            None => {
                // 規則 2: 経路が無ければつなぎ無しで切り替える。どちらでもよい向きは Left
                self.at = VState {
                    still: target.still,
                    facing: target.facing.unwrap_or(Facing::Left),
                };
                self.arrived_by = None;
            }
        }
        if let Some(i) = once {
            // 規則 5: Idle まで行ってから単発を流す。単発の始点の向きに合わせて反転する
            let facing = self
                .queue
                .back()
                .map_or(self.at.facing, |&(j, m)| self.set.clips[j].ends(m).1.facing);
            self.queue
                .push_back((i, self.set.clips[i].from.facing != facing));
            self.once_owed = false;
        }
        if !self.start_next(now) {
            self.settle(now);
        }
    }

    /// 次のつなぎ・単発があれば `now` から流し始める。
    fn start_next(&mut self, now: u64) -> bool {
        match self.queue.pop_front() {
            Some((index, mirror)) => {
                self.cur = Cur::Play {
                    index,
                    mirror,
                    start: now,
                };
                true
            }
            None => false,
        }
    }

    /// 目標の姿勢に着いた。ループがあれば流し、無ければつなぎの最後のコマかシートで止める。
    fn settle(&mut self, now: u64) {
        let next = match self.set.loop_for(self.at.still) {
            Some(index) => {
                let native = self.set.clips[index].from.facing;
                let mirror = native != Facing::Front
                    && self.at.facing != Facing::Front
                    && native != self.at.facing;
                // 同じループをもう流しているなら続ける（同じ姿勢のポーズ間の切り替えで頭から戻らない）
                if let Cur::Loop {
                    index: i,
                    mirror: m,
                    ..
                } = self.cur
                    && i == index
                    && m == mirror
                {
                    return;
                }
                Cur::Loop {
                    index,
                    mirror,
                    start: now,
                }
            }
            None => match self.arrived_by {
                Some((index, mirror)) => Cur::Hold { index, mirror },
                None => Cur::Sheet(self.desired),
            },
        };
        self.cur = next;
    }

    /// `from` から `target` へのつなぎの列。着いていれば空、行けなければ None。
    /// 本数が最少、次に反転が少ない、次に clips.json の並びが先、の順で選ぶ（規則 1）。
    fn path(&self, from: VState, target: Target) -> Option<Vec<(usize, bool)>> {
        if target.matches(from) {
            return Some(Vec::new());
        }
        let edges: Vec<(usize, bool, VState, VState)> = self
            .set
            .clips
            .iter()
            .enumerate()
            .filter(|(_, c)| c.kind == ClipKind::Transition)
            .flat_map(|(i, c)| {
                [false, true].map(|m| {
                    let (a, b) = c.ends(m);
                    (i, m, a, b)
                })
            })
            .collect();
        // 幅優先で、深さごとに全経路を集めて最良を選ぶ（クリップは十数本なので総当たりで足りる）
        let mut frontier: Vec<(VState, Vec<(usize, bool)>)> = vec![(from, Vec::new())];
        for _ in 0..MAX_PATH {
            let mut next = Vec::new();
            let mut done: Vec<Vec<(usize, bool)>> = Vec::new();
            for (state, steps) in &frontier {
                for &(i, m, a, b) in &edges {
                    if a != *state
                        || b == from
                        || steps.iter().any(|&(j, n)| self.set.clips[j].ends(n).1 == b)
                    {
                        continue;
                    }
                    let mut s = steps.clone();
                    s.push((i, m));
                    if target.matches(b) {
                        done.push(s);
                    } else {
                        next.push((b, s));
                    }
                }
            }
            if let Some(best) = done.into_iter().min_by_key(|s| {
                let mirrors = s.iter().filter(|&&(_, m)| m).count();
                let order: Vec<usize> = s.iter().map(|&(i, _)| i).collect();
                (mirrors, order)
            }) {
                return Some(best);
            }
            if next.is_empty() {
                return None;
            }
            frontier = next;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_poses_are_failed_and_loopless_stills() {
        let set = ClipSet::from_json(
            r#"{"fps":8,"clips":[
              {"name":"idle","kind":"loop","from":["idle","left"],"to":["idle","left"],"frames":4},
              {"name":"sleep","kind":"transition","from":["idle","left"],"to":["sleep","left"],"frames":4}]}"#,
        )
        .unwrap();
        let poses = sheet_poses(&set);
        assert!(
            poses.contains(&Pose::Failed)
                && poses.contains(&Pose::Sleep)
                && poses.contains(&Pose::Waiting)
        );
        assert!(!poses.contains(&Pose::Idle) && !poses.contains(&Pose::Petted));
    }

    fn mame_like() -> ClipSet {
        ClipSet::from_json(
            r#"{"fps":8,"clips":[
              {"name":"idle","kind":"loop","from":["idle","left"],"to":["idle","left"],"frames":4},
              {"name":"walk","kind":"loop","from":["walk","right"],"to":["walk","right"],"frames":4},
              {"name":"stop","kind":"transition","from":["walk","right"],"to":["idle","right"],"frames":8}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn new_with_failed_shows_the_sheet_row() {
        // 検証係の指摘（2026-10-21）: Idle のループを出していた
        let mut p = Player::new(mame_like(), Pose::Failed, 0);
        assert_eq!(p.tick(0), Show::Sheet(Pose::Failed));
        p.set_pose(Pose::Idle, 100);
        assert!(matches!(p.tick(100), Show::Clip { index: 0, .. }));
    }

    #[test]
    fn may_move_reflects_the_last_tick_not_set_pose() {
        // 検証係の指摘（2026-10-21）: set_pose の直後に変わっていた
        let mut p = Player::new(mame_like(), Pose::RunRight, 0);
        p.tick(0);
        assert!(p.may_move());
        p.set_pose(Pose::Idle, 100);
        assert!(p.may_move(), "tick までは直前の値");
        p.tick(100);
        assert!(!p.may_move(), "stop の最中は動かない");
    }

    #[test]
    fn facing_flip_keeps_front() {
        assert_eq!(Facing::Front.flip(), Facing::Front);
        assert_eq!(Facing::Left.flip(), Facing::Right);
    }
}
