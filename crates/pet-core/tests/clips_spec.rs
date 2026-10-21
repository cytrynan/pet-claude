//! `pet_core::clips` の仕様テスト（テスト先行。実装を見ずに requirements.md の「計画の規則」と
//! design.md §1 だけから書いた受入基準）。
//!
//! 共通の前提:
//! - fps は 8 なので 1 コマ = 125ms。コマ番号は `経過ms * 8 / 1000`（切り捨て）。
//! - つなぎ・単発は 8 コマ = 1000ms（`petted` だけ 16 コマ = 2000ms）。
//! - ループは idle 10 コマ(1250ms)・walk 12 コマ(1500ms)・working 6 コマ・grabbed 4 コマ。
//! - 時刻はテストごとに手で検算できる値だけを使う（各テストの先頭コメントに道筋を書く）。

use pet_core::clips::Facing::{Front, Left, Right};
use pet_core::clips::Still::{Attention, Dragged, Idle, Sleep, Walk, Working};
use pet_core::clips::{ClipDef, ClipKind, ClipSet, Facing, Player, Show, Still, VState};
use pet_core::sprite::Pose;

// ---------------------------------------------------------------------------
// 共通の道具
// ---------------------------------------------------------------------------

type Row = (
    &'static str,
    ClipKind,
    (Still, Facing),
    (Still, Facing),
    u32,
);

/// requirements.md「13 本の割り当て」を、そのままの並びで書いた表。
const MAME: [Row; 13] = [
    ("idle", ClipKind::Loop, (Idle, Left), (Idle, Left), 10),
    ("walk", ClipKind::Loop, (Walk, Right), (Walk, Right), 12),
    (
        "working",
        ClipKind::Loop,
        (Working, Left),
        (Working, Left),
        6,
    ),
    (
        "grabbed",
        ClipKind::Loop,
        (Dragged, Front),
        (Dragged, Front),
        4,
    ),
    (
        "stop",
        ClipKind::Transition,
        (Walk, Right),
        (Idle, Right),
        8,
    ),
    (
        "walk_start",
        ClipKind::Transition,
        (Idle, Right),
        (Walk, Right),
        8,
    ),
    ("turn", ClipKind::Transition, (Idle, Right), (Idle, Left), 8),
    (
        "sleep",
        ClipKind::Transition,
        (Idle, Left),
        (Sleep, Left),
        8,
    ),
    ("wake", ClipKind::Transition, (Sleep, Left), (Idle, Left), 8),
    (
        "put_down",
        ClipKind::Transition,
        (Dragged, Front),
        (Idle, Left),
        8,
    ),
    (
        "call",
        ClipKind::Transition,
        (Idle, Left),
        (Attention, Front),
        8,
    ),
    ("happy", ClipKind::Once, (Idle, Left), (Idle, Left), 8),
    ("petted", ClipKind::Once, (Idle, Left), (Idle, Left), 16),
];

fn vs(p: (Still, Facing)) -> VState {
    VState {
        still: p.0,
        facing: p.1,
    }
}

fn def(
    name: &str,
    kind: ClipKind,
    from: (Still, Facing),
    to: (Still, Facing),
    frames: u32,
) -> ClipDef {
    ClipDef {
        name: name.to_string(),
        kind,
        from: vs(from),
        to: vs(to),
        frames,
    }
}

/// まめドラゴンの 13 本を fps 8 で組み立てる。
fn mame_set() -> ClipSet {
    ClipSet {
        fps: 8,
        clips: MAME.iter().map(|r| def(r.0, r.1, r.2, r.3, r.4)).collect(),
    }
}

/// `mame_set()` の中でのクリップの番号（名前から引く）。
fn ix(name: &str) -> usize {
    mame_set()
        .clips
        .iter()
        .position(|c| c.name == name)
        .unwrap_or_else(|| panic!("クリップ {name} が無い"))
}

fn clip(name: &str, frame: u32, mirror: bool) -> Show {
    Show::Clip {
        index: ix(name),
        frame,
        mirror,
    }
}

/// 時刻 0 に (Idle, Left) の待機ループで始まるプレイヤー。
fn new_player() -> Player {
    Player::new(mame_set(), Pose::Idle, 0)
}

/// RunRight を時刻 0 に伝え、turn(0-1000) → walk_start(1000-2000) を流し終えて
/// 時刻 2000 から walk ループ（反転なし）を流しているプレイヤー。最後に tick(2000) した状態。
fn walking_right() -> Player {
    let mut p = new_player();
    p.set_pose(Pose::RunRight, 0);
    assert_eq!(
        p.tick(0),
        clip("turn", 0, true),
        "前提: turn を反転して始まる"
    );
    assert_eq!(
        p.tick(1000),
        clip("walk_start", 0, false),
        "前提: walk_start"
    );
    assert_eq!(p.tick(2000), clip("walk", 0, false), "前提: walk ループ");
    p
}

/// walking_right の 3000 に RunLeft を伝え、stop(3000-4000) → turn(4000-5000) →
/// walk_start 反転(5000-6000) を流し終えて、時刻 6000 から walk ループ（反転）を流している。
fn walking_left() -> Player {
    let mut p = walking_right();
    p.set_pose(Pose::RunLeft, 3000);
    assert_eq!(p.tick(3000), clip("stop", 0, false), "前提: stop");
    assert_eq!(
        p.tick(4000),
        clip("turn", 0, false),
        "前提: turn（反転なし）"
    );
    assert_eq!(
        p.tick(5000),
        clip("walk_start", 0, true),
        "前提: walk_start 反転"
    );
    assert_eq!(p.tick(6000), clip("walk", 0, true), "前提: walk ループ反転");
    p
}

/// walking_right の 3000 に Idle を伝え、stop(3000-4000) を流し終えて、
/// 時刻 4000 から idle ループ（反転＝向き Right）を流している。
fn idle_right() -> Player {
    let mut p = walking_right();
    p.set_pose(Pose::Idle, 3000);
    assert_eq!(p.tick(3000), clip("stop", 0, false), "前提: stop");
    assert_eq!(p.tick(4000), clip("idle", 0, true), "前提: idle ループ反転");
    p
}

/// (Idle, Left) から 500 に Sleep を伝え、sleep(500-1500) を流し終えて最後のコマで止まっている。
/// 最後に tick(5000) した状態。
fn sleeping_left() -> Player {
    let mut p = new_player();
    p.set_pose(Pose::Sleep, 500);
    assert_eq!(p.tick(500), clip("sleep", 0, false), "前提: sleep");
    assert_eq!(
        p.tick(1500),
        clip("sleep", 7, false),
        "前提: 最後のコマで止まる"
    );
    assert_eq!(p.tick(5000), clip("sleep", 7, false), "前提: 止まったまま");
    p
}

/// 1 回の tick で表示と may_move を同時に確かめる。
fn step(p: &mut Player, t: u64, expect: Show, movable: bool) {
    assert_eq!(p.tick(t), expect, "t={t}");
    assert_eq!(p.may_move(), movable, "t={t} の may_move");
}

// ---------------------------------------------------------------------------
// 規則 1・4: 最短経路とその後のループ（反転を含む）
// ---------------------------------------------------------------------------

// 規則 1・4: (Idle,Left) で RunRight → turn を反転(Idle,Left→Idle,Right) → walk_start の 2 本、
// 流し終えたら walk ループ（反転なし）。道筋: turn 0-1000、walk_start 1000-2000、walk 2000-。
#[test]
fn rule1_idle_left_to_run_right_plays_mirrored_turn_then_walk_start_then_walk() {
    let mut p = new_player();
    p.set_pose(Pose::RunRight, 0);
    assert_eq!(p.tick(0), clip("turn", 0, true));
    assert_eq!(p.tick(124), clip("turn", 0, true));
    assert_eq!(p.tick(125), clip("turn", 1, true));
    assert_eq!(p.tick(999), clip("turn", 7, true));
    assert_eq!(p.tick(1000), clip("walk_start", 0, false));
    assert_eq!(p.tick(1999), clip("walk_start", 7, false));
    assert_eq!(p.tick(2000), clip("walk", 0, false));
    assert_eq!(p.tick(2125), clip("walk", 1, false));
}

// 規則 1・4: 歩き Right の最中に Idle → stop（反転なし）→ idle ループを反転（向きが Right なので）。
// 道筋: walk 2000-、Idle を 3000 に伝える。stop 3000-4000、idle 4000-。
#[test]
fn rule1_walk_right_to_idle_plays_stop_then_mirrored_idle() {
    let mut p = walking_right();
    assert_eq!(p.tick(2500), clip("walk", 4, false));
    p.set_pose(Pose::Idle, 3000);
    assert_eq!(p.tick(3000), clip("stop", 0, false));
    assert_eq!(p.tick(3999), clip("stop", 7, false));
    assert_eq!(p.tick(4000), clip("idle", 0, true));
    assert_eq!(p.tick(4125), clip("idle", 1, true));
}

// 規則 1・4: 歩き Right の最中に RunLeft → stop → turn（反転なし）→ walk_start を反転 → walk を反転。
// 道筋: RunLeft を 3000 に伝える。stop 3000-4000、turn 4000-5000、walk_start 5000-6000、walk 6000-。
#[test]
fn rule1_walk_right_to_run_left_plays_stop_turn_mirrored_walk_start_then_mirrored_walk() {
    let mut p = walking_right();
    p.set_pose(Pose::RunLeft, 3000);
    assert_eq!(p.tick(3000), clip("stop", 0, false));
    assert_eq!(p.tick(3999), clip("stop", 7, false));
    assert_eq!(p.tick(4000), clip("turn", 0, false));
    assert_eq!(p.tick(4999), clip("turn", 7, false));
    assert_eq!(p.tick(5000), clip("walk_start", 0, true));
    assert_eq!(p.tick(5999), clip("walk_start", 7, true));
    assert_eq!(p.tick(6000), clip("walk", 0, true));
    assert_eq!(p.tick(6125), clip("walk", 1, true));
}

// 規則 1・4: 歩き Left（walk を反転して流している）から Idle → stop を反転 → idle ループ（反転なし）。
// 道筋: walking_left（walk 6000-）、Idle を 7000 に伝える。stop 7000-8000、idle 8000-。
#[test]
fn rule1_walk_left_to_idle_plays_mirrored_stop_then_unmirrored_idle() {
    let mut p = walking_left();
    p.set_pose(Pose::Idle, 7000);
    assert_eq!(p.tick(7000), clip("stop", 0, true));
    assert_eq!(p.tick(7999), clip("stop", 7, true));
    assert_eq!(p.tick(8000), clip("idle", 0, false));
}

// ---------------------------------------------------------------------------
// 規則 4: Sleep と起きる（ループが無い姿勢は最後のコマで止める）
// ---------------------------------------------------------------------------

// 規則 4: (Idle,Left) で Sleep → sleep を流し、最後のコマ(frames-1 = 7)で止まる。
// 道筋: Sleep を 500 に伝える。sleep 500-1500、以降は frame 7 のまま。
#[test]
fn rule4_sleep_from_idle_left_plays_sleep_and_holds_last_frame() {
    let mut p = new_player();
    p.set_pose(Pose::Sleep, 500);
    assert_eq!(p.tick(500), clip("sleep", 0, false));
    assert_eq!(p.tick(1000), clip("sleep", 4, false));
    assert_eq!(p.tick(1499), clip("sleep", 7, false));
    assert_eq!(p.tick(1500), clip("sleep", 7, false));
    assert_eq!(p.tick(9000), clip("sleep", 7, false));
}

// 規則 4: Sleep で止まっている状態から Idle → wake → idle ループ（向き Left なので反転なし）。
// 道筋: sleeping_left（最後に tick(5000)）、Idle を 6000 に伝える。wake 6000-7000、idle 7000-。
#[test]
fn rule4_wake_from_sleep_left_plays_wake_then_unmirrored_idle() {
    let mut p = sleeping_left();
    p.set_pose(Pose::Idle, 6000);
    assert_eq!(p.tick(6000), clip("wake", 0, false));
    assert_eq!(p.tick(6999), clip("wake", 7, false));
    assert_eq!(p.tick(7000), clip("idle", 0, false));
}

// 規則 4: (Idle,Right) で Sleep なら sleep を反転して流し、最後のコマで止まる（反転のまま）。
// 道筋: idle_right（idle 反転 4000-）、Sleep を 5000 に伝える。sleep 5000-6000、以降 frame 7。
#[test]
fn rule4_sleep_from_idle_right_plays_mirrored_sleep_and_holds_last_frame() {
    let mut p = idle_right();
    p.set_pose(Pose::Sleep, 5000);
    assert_eq!(p.tick(5000), clip("sleep", 0, true));
    assert_eq!(p.tick(5500), clip("sleep", 4, true));
    assert_eq!(p.tick(5999), clip("sleep", 7, true));
    assert_eq!(p.tick(6000), clip("sleep", 7, true));
    assert_eq!(p.tick(9000), clip("sleep", 7, true));
}

// 規則 4: 反転した sleep で止まっている（Sleep, Right）から Idle → wake を反転 → idle ループを反転。
// 道筋: 上の続き。Idle を 9500 に伝える。wake 9500-10500、idle 10500-。
#[test]
fn rule4_wake_from_sleep_right_plays_mirrored_wake_then_mirrored_idle() {
    let mut p = idle_right();
    p.set_pose(Pose::Sleep, 5000);
    assert_eq!(p.tick(9000), clip("sleep", 7, true));
    p.set_pose(Pose::Idle, 9500);
    assert_eq!(p.tick(9500), clip("wake", 0, true));
    assert_eq!(p.tick(10499), clip("wake", 7, true));
    assert_eq!(p.tick(10500), clip("idle", 0, true));
}

// 規則 4・設計「最初は〜つなぎ無しで置く」: new で Sleep を渡すとつなぎ無しで Sleep に置かれ、
// ループが無い姿勢なのでシートの行で表示される（向きは Left）。そこから Idle なら wake が流れる。
#[test]
fn rule4_new_with_sleep_shows_sheet_row_and_can_wake() {
    let mut p = Player::new(mame_set(), Pose::Sleep, 0);
    assert_eq!(p.tick(0), Show::Sheet(Pose::Sleep));
    assert_eq!(p.tick(500), Show::Sheet(Pose::Sleep));
    p.set_pose(Pose::Idle, 1000);
    assert_eq!(p.tick(1000), clip("wake", 0, false));
    assert_eq!(p.tick(2000), clip("idle", 0, false));
}

// ---------------------------------------------------------------------------
// 規則 6: Grabbed の割り込み、離した後
// ---------------------------------------------------------------------------

// 規則 6: つなぎ（turn）の最中でも Grabbed は次の tick で grabbed ループ（反転なし・向き Front）。
// 道筋: RunRight を 0 に伝え turn 再生中の 300 に Grabbed。grabbed は 300 から。
//   625ms 経過 → コマ 5 → 4 コマなので 5 mod 4 = 1。
#[test]
fn rule6_grabbed_interrupts_transition_immediately() {
    let mut p = new_player();
    p.set_pose(Pose::RunRight, 0);
    assert_eq!(p.tick(300), clip("turn", 2, true));
    p.set_pose(Pose::Grabbed, 300);
    step(&mut p, 300, clip("grabbed", 0, false), false);
    step(&mut p, 925, clip("grabbed", 1, false), false);
}

// 規則 6: 単発（happy）の最中でも Grabbed はすぐ grabbed ループに切り替わる。
// 道筋: Jumping を 0 に伝え、400（happy のコマ 3）で Grabbed。
#[test]
fn rule6_grabbed_interrupts_once_immediately() {
    let mut p = new_player();
    p.set_pose(Pose::Jumping, 0);
    assert_eq!(p.tick(400), clip("happy", 3, false));
    p.set_pose(Pose::Grabbed, 400);
    assert_eq!(p.tick(400), clip("grabbed", 0, false));
    assert_eq!(p.tick(900), clip("grabbed", 0, false)); // 500ms → コマ 4 → 4 mod 4 = 0
}

// 規則 6・3・4: 離して Idle → put_down（Dragged,Front→Idle,Left）→ idle ループ（反転なし）。
// 向きが Front なので「どちらでもよい」。道筋: grabbed を 300 から流し、Idle を 2000 に伝える。
//   put_down 2000-3000、idle 3000-。
#[test]
fn rule6_release_from_grabbed_plays_put_down_then_unmirrored_idle() {
    let mut p = new_player();
    p.set_pose(Pose::RunRight, 0);
    assert_eq!(p.tick(300), clip("turn", 2, true));
    p.set_pose(Pose::Grabbed, 300);
    assert_eq!(p.tick(925), clip("grabbed", 1, false));
    p.set_pose(Pose::Idle, 2000);
    assert_eq!(p.tick(2000), clip("put_down", 0, false));
    assert_eq!(p.tick(2999), clip("put_down", 7, false));
    assert_eq!(p.tick(3000), clip("idle", 0, false));
}

// 規則 3: 向きが Front（Grabbed 後）の Sleep は「どちらでもよい」ので、向きを問わず最短の列を選ぶ。
// (Dragged,Front) → put_down → (Idle,Left) → sleep → (Sleep,Left) の 2 本。
// 道筋: new(Grabbed) の 1000 に Sleep。put_down 1000-2000、sleep 2000-3000、以降 sleep の frame 7。
#[test]
fn rule3_sleep_from_grabbed_routes_put_down_then_sleep() {
    let mut p = Player::new(mame_set(), Pose::Grabbed, 0);
    assert_eq!(p.tick(0), clip("grabbed", 0, false));
    p.set_pose(Pose::Sleep, 1000);
    assert_eq!(p.tick(1000), clip("put_down", 0, false));
    assert_eq!(p.tick(1999), clip("put_down", 7, false));
    assert_eq!(p.tick(2000), clip("sleep", 0, false));
    assert_eq!(p.tick(3000), clip("sleep", 7, false));
    assert_eq!(p.tick(8000), clip("sleep", 7, false));
}

// ---------------------------------------------------------------------------
// 規則 4・6: Waiting / Waving（Attention はループが無い）
// ---------------------------------------------------------------------------

// 規則 4: (Idle,Left) で Waiting → call を流し、最後のコマ(7)で止まる。
// 道筋: Waiting を 0 に伝える。call 0-1000、以降 frame 7。
#[test]
fn rule4_waiting_from_idle_left_plays_call_and_holds_last_frame() {
    let mut p = new_player();
    p.set_pose(Pose::Waiting, 0);
    assert_eq!(p.tick(0), clip("call", 0, false));
    assert_eq!(p.tick(500), clip("call", 4, false));
    assert_eq!(p.tick(1000), clip("call", 7, false));
    assert_eq!(p.tick(5000), clip("call", 7, false));
}

// 規則 1・4: 歩き Right からの Waiting → stop → call を反転（Idle,Right→Attention,Front）、止まる。
// 道筋: Waiting を 3000 に伝える。stop 3000-4000、call 4000-5000（反転）、以降 frame 7（反転のまま）。
#[test]
fn rule4_waiting_from_walk_right_plays_stop_then_mirrored_call_and_holds() {
    let mut p = walking_right();
    p.set_pose(Pose::Waiting, 3000);
    assert_eq!(p.tick(3000), clip("stop", 0, false));
    assert_eq!(p.tick(4000), clip("call", 0, true));
    assert_eq!(p.tick(4500), clip("call", 4, true));
    assert_eq!(p.tick(5000), clip("call", 7, true));
    assert_eq!(p.tick(8000), clip("call", 7, true));
}

// 計画の規則（ポーズ→目標）: Waving も Waiting と同じ目標 (Attention, Front)。
#[test]
fn waving_is_planned_like_waiting() {
    let mut p = new_player();
    p.set_pose(Pose::Waving, 0);
    assert_eq!(p.tick(0), clip("call", 0, false));
    assert_eq!(p.tick(1000), clip("call", 7, false));
}

// 規則 2・3 と設計「Left を選ぶ」: Attention から Idle へのつなぎは無い（非スコープ）ので、
// つなぎ無しで切り替える。向きは Front → 「どちらでもよい」→ Left、idle ループ（反転なし）が切替時刻から。
// 道筋: call で止まっている状態の 2000 に Idle。
#[test]
fn rule2_idle_from_attention_switches_to_unmirrored_idle_without_transition() {
    let mut p = new_player();
    p.set_pose(Pose::Waiting, 0);
    assert_eq!(p.tick(1000), clip("call", 7, false));
    p.set_pose(Pose::Idle, 2000);
    assert_eq!(p.tick(2000), clip("idle", 0, false));
    assert_eq!(p.tick(2125), clip("idle", 1, false));
}

// 規則 2: Attention から RunRight への経路も無いので、つなぎ無しで (Walk,Right) の walk ループ
// （反転なし）へ切り替える。切替時刻 2000 から。
#[test]
fn rule2_run_right_from_attention_switches_to_walk_loop_without_transition() {
    let mut p = new_player();
    p.set_pose(Pose::Waiting, 0);
    assert_eq!(p.tick(1000), clip("call", 7, false));
    p.set_pose(Pose::RunRight, 2000);
    assert_eq!(p.tick(2000), clip("walk", 0, false));
    assert_eq!(p.tick(2250), clip("walk", 2, false));
}

// ---------------------------------------------------------------------------
// 規則 2・4: Running / Review（つなぎが無いので working ループへ切り替え）
// ---------------------------------------------------------------------------

// 規則 2・4: (Idle,Left) で Running → つなぎ無しで working ループ（反転なし）へ切り替わる。
// 道筋: Running を 500 に伝える。working は 500 から（6 コマ）。
//   tick(1250): 750ms → コマ 6 → 6 mod 6 = 0。tick(1375): 875ms → コマ 7 → 1。
#[test]
fn rule2_running_from_idle_left_switches_to_working_loop() {
    let mut p = new_player();
    p.set_pose(Pose::Running, 500);
    assert_eq!(p.tick(500), clip("working", 0, false));
    assert_eq!(p.tick(1250), clip("working", 0, false));
    assert_eq!(p.tick(1375), clip("working", 1, false));
}

// 規則 2・4: 向き Right（歩き Right）から Running → stop は挟まず、つなぎ無しで working ループを
// 反転して切り替える（目標は (Working, 今の向き=Right)）。切替時刻 3000 から。
#[test]
fn rule2_running_from_walk_right_switches_to_mirrored_working_loop() {
    let mut p = walking_right();
    p.set_pose(Pose::Running, 3000);
    assert_eq!(p.tick(3000), clip("working", 0, true));
    assert_eq!(p.tick(3125), clip("working", 1, true));
}

// 計画の規則（ポーズ→目標）: Review も Running と同じ目標 (Working, 今の向き)。
#[test]
fn review_is_planned_like_running() {
    let mut p = new_player();
    p.set_pose(Pose::Review, 500);
    assert_eq!(p.tick(500), clip("working", 0, false));
    assert_eq!(p.tick(1375), clip("working", 1, false));
}

// 規則 2・4: 反転した working（Working,Right）から Idle → 経路が無いので、目標 (Idle, Right) へ
// つなぎ無しで切り替え、idle ループを反転して流す（向きは保たれる）。切替時刻 4000 から。
#[test]
fn rule2_idle_from_mirrored_working_switches_to_mirrored_idle() {
    let mut p = walking_right();
    p.set_pose(Pose::Running, 3000);
    assert_eq!(p.tick(3000), clip("working", 0, true));
    p.set_pose(Pose::Idle, 4000);
    assert_eq!(p.tick(4000), clip("idle", 0, true));
}

// ---------------------------------------------------------------------------
// 規則 5: 単発（Jumping・Petted）
// ---------------------------------------------------------------------------

// 規則 5・4: (Idle,Left) で Jumping → happy を 1 回だけ流し、終わってもポーズが同じなら idle ループ。
// 道筋: Jumping を 0 に伝える。happy 0-1000、idle 1000-。
#[test]
fn rule5_jumping_from_idle_left_plays_happy_once_then_idle_loop() {
    let mut p = new_player();
    p.set_pose(Pose::Jumping, 0);
    assert_eq!(p.tick(0), clip("happy", 0, false));
    assert_eq!(p.tick(500), clip("happy", 4, false));
    assert_eq!(p.tick(999), clip("happy", 7, false));
    assert_eq!(p.tick(1000), clip("idle", 0, false));
    assert_eq!(p.tick(1125), clip("idle", 1, false));
}

// 規則 5・4: Petted も同じ。petted は 16 コマ = 2000ms。
// 道筋: Petted を 0 に伝える。petted 0-2000、idle 2000-。
#[test]
fn rule5_petted_from_idle_left_plays_petted_once_then_idle_loop() {
    let mut p = new_player();
    p.set_pose(Pose::Petted, 0);
    assert_eq!(p.tick(0), clip("petted", 0, false));
    assert_eq!(p.tick(1000), clip("petted", 8, false));
    assert_eq!(p.tick(1999), clip("petted", 15, false));
    assert_eq!(p.tick(2000), clip("idle", 0, false));
    assert_eq!(p.tick(2125), clip("idle", 1, false));
}

// 規則 5: 歩き Right からの Jumping → stop → happy を反転 → idle ループを反転。
// 道筋: Jumping を 3000 に伝える。stop 3000-4000、happy 4000-5000（反転）、idle 5000-（反転）。
#[test]
fn rule5_jumping_from_walk_right_plays_stop_then_mirrored_happy_then_mirrored_idle() {
    let mut p = walking_right();
    p.set_pose(Pose::Jumping, 3000);
    assert_eq!(p.tick(3000), clip("stop", 0, false));
    assert_eq!(p.tick(3999), clip("stop", 7, false));
    assert_eq!(p.tick(4000), clip("happy", 0, true));
    assert_eq!(p.tick(4999), clip("happy", 7, true));
    assert_eq!(p.tick(5000), clip("idle", 0, true));
    assert_eq!(p.tick(5125), clip("idle", 1, true));
}

// 規則 5: 歩き Right からの Petted → stop → petted を反転 → idle ループを反転。
// 道筋: stop 3000-4000、petted 4000-6000（反転）、idle 6000-（反転）。
#[test]
fn rule5_petted_from_walk_right_plays_stop_then_mirrored_petted_then_mirrored_idle() {
    let mut p = walking_right();
    p.set_pose(Pose::Petted, 3000);
    assert_eq!(p.tick(3000), clip("stop", 0, false));
    assert_eq!(p.tick(4000), clip("petted", 0, true));
    assert_eq!(p.tick(5999), clip("petted", 15, true));
    assert_eq!(p.tick(6000), clip("idle", 0, true));
}

// 設計「単発は、そのポーズに入るたびに 1 回だけ流す」: Jumping → 終わって idle → Idle → もう一度
// Jumping で、また happy が最初から流れる。道筋: 1 回目 0-1000、2 回目は 2000 に伝えて 2000-3000。
#[test]
fn once_plays_again_each_time_the_pose_is_entered() {
    let mut p = new_player();
    p.set_pose(Pose::Jumping, 0);
    assert_eq!(p.tick(1000), clip("idle", 0, false));
    p.set_pose(Pose::Idle, 1500);
    p.tick(1500);
    p.set_pose(Pose::Jumping, 2000);
    assert_eq!(p.tick(2000), clip("happy", 0, false));
    assert_eq!(p.tick(2999), clip("happy", 7, false));
    assert_eq!(p.tick(3000), clip("idle", 0, false));
}

// ---------------------------------------------------------------------------
// 規則 9（Failed）
// ---------------------------------------------------------------------------

// 計画の規則（ポーズ→目標）・設計: Failed はクリップを使わず Show::Sheet(Pose::Failed)。
// ループの最中なのでその場で切り替わる。
#[test]
fn failed_shows_sheet_row() {
    let mut p = new_player();
    assert_eq!(p.tick(0), clip("idle", 0, false));
    p.set_pose(Pose::Failed, 500);
    assert_eq!(p.tick(500), Show::Sheet(Pose::Failed));
    assert_eq!(p.tick(2000), Show::Sheet(Pose::Failed));
}

// 設計「Failed は見た目の状態（VState）を変えない」: 歩き Right → Failed → Idle なら、
// Failed の前の (Walk,Right) から計画する。stop が流れ、idle ループは反転。
// 道筋: Failed を 3000、Idle を 4000 に伝える。stop 4000-5000、idle 5000-（反転）。
#[test]
fn failed_keeps_previous_vstate_for_planning_back_to_idle() {
    let mut p = walking_right();
    p.set_pose(Pose::Failed, 3000);
    assert_eq!(p.tick(3000), Show::Sheet(Pose::Failed));
    assert_eq!(p.tick(3500), Show::Sheet(Pose::Failed));
    p.set_pose(Pose::Idle, 4000);
    assert_eq!(p.tick(4000), clip("stop", 0, false));
    assert_eq!(p.tick(5000), clip("idle", 0, true));
}

// ---------------------------------------------------------------------------
// 規則 7・8: 割り込み（Grabbed 以外は流しているつなぎ・単発を最後まで待つ）
// ---------------------------------------------------------------------------

// 規則 7: つなぎ（turn）の最中に Idle に変わっても、turn は最後まで流す。終点 (Idle,Right) から
// 計画し直すので idle ループは反転。walk_start は流れない。
// 道筋: RunRight を 0、Idle を 300 に伝える。turn 0-1000、idle 1000-（反転）。
#[test]
fn rule7_pose_change_during_transition_waits_until_it_ends() {
    let mut p = new_player();
    p.set_pose(Pose::RunRight, 0);
    assert_eq!(p.tick(300), clip("turn", 2, true));
    p.set_pose(Pose::Idle, 300);
    assert_eq!(p.tick(300), clip("turn", 2, true));
    assert_eq!(p.tick(999), clip("turn", 7, true));
    assert_eq!(p.tick(1000), clip("idle", 0, true));
    assert_eq!(p.tick(1125), clip("idle", 1, true));
}

// 規則 7: 単発（happy）の最中に Petted に変わっても、happy は最後まで流してから petted を流す。
// 道筋: Jumping を 0、Petted を 400 に伝える。happy 0-1000、petted 1000-3000、idle 3000-。
#[test]
fn rule7_pose_change_during_once_waits_until_it_ends() {
    let mut p = new_player();
    p.set_pose(Pose::Jumping, 0);
    assert_eq!(p.tick(400), clip("happy", 3, false));
    p.set_pose(Pose::Petted, 400);
    assert_eq!(p.tick(999), clip("happy", 7, false));
    assert_eq!(p.tick(1000), clip("petted", 0, false));
    assert_eq!(p.tick(2999), clip("petted", 15, false));
    assert_eq!(p.tick(3000), clip("idle", 0, false));
}

// 規則 8: つなぎの最中に 2 回ポーズが変わったら、計画し直すときは最後のポーズを使う。
// RunRight → (RunLeft → Sleep)。turn 終点 (Idle,Right) から Sleep なので sleep を反転して流す。
// 道筋: RunLeft を 200、Sleep を 400 に伝える。turn 0-1000、sleep 1000-2000（反転）、以降 frame 7。
#[test]
fn rule8_multiple_pose_changes_during_transition_use_the_last_one_sleep() {
    let mut p = new_player();
    p.set_pose(Pose::RunRight, 0);
    assert_eq!(p.tick(100), clip("turn", 0, true));
    p.set_pose(Pose::RunLeft, 200);
    p.set_pose(Pose::Sleep, 400);
    assert_eq!(p.tick(999), clip("turn", 7, true));
    assert_eq!(p.tick(1000), clip("sleep", 0, true));
    assert_eq!(p.tick(1500), clip("sleep", 4, true));
    assert_eq!(p.tick(2000), clip("sleep", 7, true));
}

// 規則 8: 順序を逆にして（Sleep → RunLeft）最後のポーズが RunLeft の場合。
// turn 終点 (Idle,Right) から (Walk,Left) へは turn（反転なし）→ walk_start 反転 → walk 反転。
// 道筋: Sleep を 200、RunLeft を 400 に伝える。turn 0-1000（反転）、turn 1000-2000、
//   walk_start 2000-3000（反転）、walk 3000-（反転）。
#[test]
fn rule8_multiple_pose_changes_during_transition_use_the_last_one_run_left() {
    let mut p = new_player();
    p.set_pose(Pose::RunRight, 0);
    assert_eq!(p.tick(100), clip("turn", 0, true));
    p.set_pose(Pose::Sleep, 200);
    p.set_pose(Pose::RunLeft, 400);
    assert_eq!(p.tick(999), clip("turn", 7, true));
    assert_eq!(p.tick(1000), clip("turn", 0, false));
    assert_eq!(p.tick(1999), clip("turn", 7, false));
    assert_eq!(p.tick(2000), clip("walk_start", 0, true));
    assert_eq!(p.tick(2999), clip("walk_start", 7, true));
    assert_eq!(p.tick(3000), clip("walk", 0, true));
}

// 設計「毎 tick 呼んでよい。同じポーズなら何もしない」: 同じポーズをもう一度伝えても
// 再生中のつなぎは最初からやり直されない。
// 道筋: RunRight を 0 と 500 に伝える。turn 0-1000 のまま、tick(500) はコマ 4。
#[test]
fn set_same_pose_during_transition_does_not_restart() {
    let mut p = new_player();
    p.set_pose(Pose::RunRight, 0);
    assert_eq!(p.tick(250), clip("turn", 2, true));
    p.set_pose(Pose::RunRight, 500);
    assert_eq!(p.tick(500), clip("turn", 4, true));
    assert_eq!(p.tick(1000), clip("walk_start", 0, false));
    assert_eq!(p.tick(2000), clip("walk", 0, false));
}

// 設計「同じポーズなら何もしない」: ループの最中に同じポーズを伝えてもループは続く。
// 道筋: idle ループは 0 から。Idle を 600 に伝えても tick(600) は 600ms → コマ 4。
#[test]
fn set_same_pose_during_loop_does_not_restart() {
    let mut p = new_player();
    assert_eq!(p.tick(0), clip("idle", 0, false));
    p.set_pose(Pose::Idle, 600);
    assert_eq!(p.tick(600), clip("idle", 4, false));
}

// ---------------------------------------------------------------------------
// 規則 9 と設計「時刻の決め方」
// ---------------------------------------------------------------------------

// 設計「ループの最中に set_pose でポーズが変わったら、次のクリップはその now_ms から始まる」。
// 道筋: idle ループの最中の 600 に RunRight。turn は 600 から（コマ 0）、725 でコマ 1。
#[test]
fn time_clip_after_loop_starts_at_the_set_pose_time() {
    let mut p = new_player();
    assert_eq!(p.tick(0), clip("idle", 0, false));
    assert_eq!(p.tick(500), clip("idle", 4, false));
    p.set_pose(Pose::RunRight, 600);
    assert_eq!(p.tick(600), clip("turn", 0, true));
    assert_eq!(p.tick(724), clip("turn", 0, true));
    assert_eq!(p.tick(725), clip("turn", 1, true));
}

// 設計「つなぎの次のクリップは終わった時刻から始まる（tick を呼んだ時刻ではない）」:
// tick を 1300 まで呼ばなくても、turn は 1000 に終わっており walk_start は 1000 から始まっている。
// 300ms 経過 → コマ 2。
#[test]
fn time_next_clip_starts_at_end_time_even_when_tick_is_late() {
    let mut p = new_player();
    p.set_pose(Pose::RunRight, 0);
    assert_eq!(p.tick(1300), clip("walk_start", 2, false));
}

// 設計「1 回の tick で複数のクリップの終わりを越えたら、越えた分だけ順に進める」:
// turn(0-1000)・walk_start(1000-2000) を越えて、walk は 2000 から。2300 は 300ms → コマ 2。
#[test]
fn time_tick_crossing_two_clip_ends_advances_through_both() {
    let mut p = new_player();
    p.set_pose(Pose::RunRight, 0);
    assert_eq!(p.tick(2300), clip("walk", 2, false));
}

// 同上、単発を含む 3 本越え: 歩き Right の 3000 に Jumping。stop(3000-4000)・happy(4000-5000)
// を越えて idle ループ（反転）は 5000 から。5300 は 300ms → コマ 2。
#[test]
fn time_tick_crossing_transition_and_once_advances_through_both() {
    let mut p = walking_right();
    p.set_pose(Pose::Jumping, 3000);
    assert_eq!(p.tick(5300), clip("idle", 2, true));
}

// 規則 9: ループのコマ番号は `(経過ms × fps / 1000) mod コマ数`。idle は 10 コマ(1250ms 周期)。
// 124ms → 0、125 → 1、249 → 1、250 → 2、1249 → 9、1250 → 10 mod 10 = 0、1375 → 11 mod 10 = 1。
#[test]
fn rule9_loop_frame_follows_elapsed_formula() {
    let mut p = new_player();
    assert_eq!(p.tick(0), clip("idle", 0, false));
    assert_eq!(p.tick(124), clip("idle", 0, false));
    assert_eq!(p.tick(125), clip("idle", 1, false));
    assert_eq!(p.tick(249), clip("idle", 1, false));
    assert_eq!(p.tick(250), clip("idle", 2, false));
    assert_eq!(p.tick(1249), clip("idle", 9, false));
    assert_eq!(p.tick(1250), clip("idle", 0, false));
    assert_eq!(p.tick(1375), clip("idle", 1, false));
}

// 規則 9: 長い時間たっても剰余で回る。idle: 100000ms → 800 mod 10 = 0。
// walk（12 コマ = 1500ms 周期、2000 開始）: 3500 は 1500ms → 12 mod 12 = 0、
// 5625 は 3625ms → コマ 29 → 29 mod 12 = 5。
#[test]
fn rule9_loop_frame_wraps_by_modulo() {
    let mut p = new_player();
    assert_eq!(p.tick(100_000), clip("idle", 0, false));

    let mut w = walking_right();
    assert_eq!(w.tick(3500), clip("walk", 0, false));
    assert_eq!(w.tick(5625), clip("walk", 5, false));
}

// 規則 9: つなぎのコマ番号は `min(経過ms × fps / 1000, コマ数 − 1)`。
// turn は 8 コマ: 875ms → 7、999ms → 7（頭打ちの手前）、1000ms で終わって次のクリップになる。
#[test]
fn rule9_transition_frame_is_elapsed_formula_clamped_and_ends_at_frames_times_period() {
    let mut p = new_player();
    p.set_pose(Pose::RunRight, 0);
    assert_eq!(p.tick(0), clip("turn", 0, true));
    assert_eq!(p.tick(374), clip("turn", 2, true));
    assert_eq!(p.tick(375), clip("turn", 3, true));
    assert_eq!(p.tick(874), clip("turn", 6, true));
    assert_eq!(p.tick(875), clip("turn", 7, true));
    assert_eq!(p.tick(999), clip("turn", 7, true));
    assert_eq!(p.tick(1000), clip("walk_start", 0, false));
}

// 設計 API: `clips()` は new に渡した ClipSet をそのまま返す。
#[test]
fn clips_returns_the_set_given_to_new() {
    let p = new_player();
    assert_eq!(p.clips(), &mame_set());
}

// ---------------------------------------------------------------------------
// 規則 10: may_move
// ---------------------------------------------------------------------------

// 規則 10: 窓を横に動かしてよいのは Walk のループを流している間だけ。idle・turn・walk_start では
// false、walk ループで true、stop の最中は false、idle に戻ると false。
// 道筋: RunRight を 0 に伝える。turn 0-1000、walk_start 1000-2000、walk 2000-。Idle を 3000 に伝える。
//   stop 3000-4000、idle 4000-。
#[test]
fn rule10_may_move_is_true_only_while_walk_loop_plays() {
    let mut p = new_player();
    step(&mut p, 0, clip("idle", 0, false), false);
    p.set_pose(Pose::RunRight, 0);
    step(&mut p, 0, clip("turn", 0, true), false);
    step(&mut p, 999, clip("turn", 7, true), false);
    step(&mut p, 1000, clip("walk_start", 0, false), false);
    step(&mut p, 1999, clip("walk_start", 7, false), false);
    step(&mut p, 2000, clip("walk", 0, false), true);
    step(&mut p, 2999, clip("walk", 7, false), true);
    p.set_pose(Pose::Idle, 3000);
    step(&mut p, 3000, clip("stop", 0, false), false);
    step(&mut p, 3999, clip("stop", 7, false), false);
    step(&mut p, 4000, clip("idle", 0, true), false);
}

// 規則 10: 反転した walk ループ（歩き Left）でも true。
#[test]
fn rule10_may_move_is_true_for_mirrored_walk_loop() {
    let mut p = walking_left();
    step(&mut p, 6000, clip("walk", 0, true), true);
    step(&mut p, 6500, clip("walk", 4, true), true);
}

// 規則 10: 止まったコマ（sleep・call の最後のコマ）・grabbed・working・単発では false。
#[test]
fn rule10_may_move_is_false_for_hold_grabbed_working_and_once() {
    let mut s = sleeping_left();
    step(&mut s, 6000, clip("sleep", 7, false), false);

    let mut c = new_player();
    c.set_pose(Pose::Waiting, 0);
    step(&mut c, 2000, clip("call", 7, false), false);

    let mut g = new_player();
    g.set_pose(Pose::Grabbed, 0);
    step(&mut g, 0, clip("grabbed", 0, false), false);

    let mut w = new_player();
    w.set_pose(Pose::Running, 0);
    step(&mut w, 0, clip("working", 0, false), false);

    let mut j = new_player();
    j.set_pose(Pose::Jumping, 0);
    step(&mut j, 500, clip("happy", 4, false), false);
}

// ---------------------------------------------------------------------------
// 規則 1（同数のときの選び方）: 反転の少ない方、それも同じなら clips.json の並びが先の方
// ---------------------------------------------------------------------------

/// 経路の同数比較用の小さなセット。put_left・put_right は Dragged,Front → Idle,(Left|Right) の
/// 1 本ずつ。`left_first` で並びの先後を入れ替える。
fn put_down_tie_set(left_first: bool) -> ClipSet {
    let idle = def("idle", ClipKind::Loop, (Idle, Left), (Idle, Left), 8);
    let grabbed = def(
        "grabbed",
        ClipKind::Loop,
        (Dragged, Front),
        (Dragged, Front),
        4,
    );
    let put_left = def(
        "put_left",
        ClipKind::Transition,
        (Dragged, Front),
        (Idle, Left),
        8,
    );
    let put_right = def(
        "put_right",
        ClipKind::Transition,
        (Dragged, Front),
        (Idle, Right),
        8,
    );
    let (a, b) = if left_first {
        (put_left, put_right)
    } else {
        (put_right, put_left)
    };
    ClipSet {
        fps: 8,
        clips: vec![idle, grabbed, a, b],
    }
}

// 規則 1・3: 本数も反転数も同じなら clips.json の並びが先のクリップ。Front からの Idle は向きを
// 問わないので、put_left も put_right も反転なしで使え、並びが先の方を選ぶ。
// 並びが [idle, grabbed, put_left, put_right] → put_left(index 2)、着いた (Idle,Left) で idle 反転なし。
// 道筋: new(Grabbed)、Idle を 1000 に伝える。put_left 1000-2000、idle 2000-。
#[test]
fn rule1_tie_prefers_earlier_clip_in_set_left_first() {
    let mut p = Player::new(put_down_tie_set(true), Pose::Grabbed, 0);
    p.set_pose(Pose::Idle, 1000);
    assert_eq!(
        p.tick(1000),
        Show::Clip {
            index: 2,
            frame: 0,
            mirror: false
        }
    );
    assert_eq!(
        p.tick(2000),
        Show::Clip {
            index: 0,
            frame: 0,
            mirror: false
        }
    );
}

// 規則 1・3: 並びを [idle, grabbed, put_right, put_left] にすると put_right(index 2) が選ばれ、
// 着いた (Idle,Right) なので idle ループは反転。
#[test]
fn rule1_tie_prefers_earlier_clip_in_set_right_first() {
    let mut p = Player::new(put_down_tie_set(false), Pose::Grabbed, 0);
    p.set_pose(Pose::Idle, 1000);
    assert_eq!(
        p.tick(1000),
        Show::Clip {
            index: 2,
            frame: 0,
            mirror: false
        }
    );
    assert_eq!(
        p.tick(2000),
        Show::Clip {
            index: 0,
            frame: 0,
            mirror: true
        }
    );
}

// 規則 1: 本数が同じなら反転の少ない方を、並びの先後より優先する。
// セット [idle, walk, ws_mirrored, ws_direct]:
//   ws_mirrored = (Idle,Right)→(Walk,Left)（反転すると (Idle,Left)→(Walk,Right) で 1 本・反転 1）
//   ws_direct   = (Idle,Left)→(Walk,Right)（反転なしで 1 本）
// (Idle,Left) から RunRight は、並びが後の ws_direct(index 3) を反転なしで使う。
// 道筋: RunRight を 0 に伝える。ws_direct 0-1000、walk 1000-。
#[test]
fn rule1_tie_prefers_fewer_mirrors_over_earlier_position() {
    let set = ClipSet {
        fps: 8,
        clips: vec![
            def("idle", ClipKind::Loop, (Idle, Left), (Idle, Left), 8),
            def("walk", ClipKind::Loop, (Walk, Right), (Walk, Right), 8),
            def(
                "ws_mirrored",
                ClipKind::Transition,
                (Idle, Right),
                (Walk, Left),
                8,
            ),
            def(
                "ws_direct",
                ClipKind::Transition,
                (Idle, Left),
                (Walk, Right),
                8,
            ),
        ],
    };
    let mut p = Player::new(set, Pose::Idle, 0);
    p.set_pose(Pose::RunRight, 0);
    assert_eq!(
        p.tick(0),
        Show::Clip {
            index: 3,
            frame: 0,
            mirror: false
        }
    );
    assert_eq!(
        p.tick(1000),
        Show::Clip {
            index: 1,
            frame: 0,
            mirror: false
        }
    );
}

// ---------------------------------------------------------------------------
// ClipSet::from_json
// ---------------------------------------------------------------------------

fn still_name(s: Still) -> &'static str {
    match s {
        Idle => "idle",
        Walk => "walk",
        Sleep => "sleep",
        Dragged => "dragged",
        Attention => "attention",
        Working => "working",
    }
}

fn facing_name(f: Facing) -> &'static str {
    match f {
        Left => "left",
        Right => "right",
        Front => "front",
    }
}

fn kind_name(k: ClipKind) -> &'static str {
    match k {
        ClipKind::Loop => "loop",
        ClipKind::Transition => "transition",
        ClipKind::Once => "once",
    }
}

/// clips.json の 1 本分（文字列で持つので、壊れた値も作れる）。
#[derive(Clone)]
struct JsonRow {
    name: String,
    kind: &'static str,
    from: [&'static str; 2],
    to: [&'static str; 2],
    frames: u32,
}

fn json_rows() -> Vec<JsonRow> {
    MAME.iter()
        .map(|r| JsonRow {
            name: r.0.to_string(),
            kind: kind_name(r.1),
            from: [still_name((r.2).0), facing_name((r.2).1)],
            to: [still_name((r.3).0), facing_name((r.3).1)],
            frames: r.4,
        })
        .collect()
}

fn rows_json(fps: u32, rows: &[JsonRow]) -> String {
    let body: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                r#"{{"name": "{}", "kind": "{}", "from": ["{}", "{}"], "to": ["{}", "{}"], "frames": {}}}"#,
                r.name, r.kind, r.from[0], r.from[1], r.to[0], r.to[1], r.frames
            )
        })
        .collect();
    format!(r#"{{"fps": {}, "clips": [{}]}}"#, fps, body.join(", "))
}

fn row_mut<'a>(rows: &'a mut [JsonRow], name: &str) -> &'a mut JsonRow {
    rows.iter_mut()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("行 {name} が無い"))
}

// from_json: 正しい JSON（13 本すべて）は読める。fps・各クリップの kind・from・to・frames が
// 表のとおりで、並びも保たれる。
#[test]
fn from_json_reads_valid_clips_json() {
    let text = rows_json(8, &json_rows());
    let set = ClipSet::from_json(&text).expect("正しい JSON は読める");
    assert_eq!(set, mame_set());
    assert_eq!(set.fps, 8);
    assert_eq!(set.clips.len(), 13);
    assert_eq!(
        set.clips[ix("stop")],
        def(
            "stop",
            ClipKind::Transition,
            (Walk, Right),
            (Idle, Right),
            8
        )
    );
    assert_eq!(set.clips[ix("petted")].kind, ClipKind::Once);
    assert_eq!(set.clips[ix("grabbed")].from, vs((Dragged, Front)));
}

// from_json: fps が 0 は Err。
#[test]
fn from_json_rejects_zero_fps() {
    let text = rows_json(0, &json_rows());
    assert!(ClipSet::from_json(&text).is_err());
}

// from_json: frames が 0 のクリップがあると Err。
#[test]
fn from_json_rejects_zero_frames() {
    let mut rows = json_rows();
    row_mut(&mut rows, "stop").frames = 0;
    assert!(ClipSet::from_json(&rows_json(8, &rows)).is_err());
}

// from_json: 名前の重複は Err。
#[test]
fn from_json_rejects_duplicate_names() {
    let mut rows = json_rows();
    let mut dup = rows[0].clone();
    dup.frames = 5;
    rows.push(dup);
    assert!(ClipSet::from_json(&rows_json(8, &rows)).is_err());
}

// from_json: ループで from≠to は Err。
#[test]
fn from_json_rejects_loop_with_different_from_and_to() {
    let mut rows = json_rows();
    row_mut(&mut rows, "idle").to = ["idle", "right"];
    assert!(ClipSet::from_json(&rows_json(8, &rows)).is_err());
}

// from_json: 単発で from≠to は Err。
#[test]
fn from_json_rejects_once_with_different_from_and_to() {
    let mut rows = json_rows();
    row_mut(&mut rows, "happy").to = ["idle", "right"];
    assert!(ClipSet::from_json(&rows_json(8, &rows)).is_err());
}

// from_json: つなぎで from＝to は Err。
#[test]
fn from_json_rejects_transition_with_same_from_and_to() {
    let mut rows = json_rows();
    let from = row_mut(&mut rows, "stop").from;
    row_mut(&mut rows, "stop").to = from;
    assert!(ClipSet::from_json(&rows_json(8, &rows)).is_err());
}

// from_json: Dragged の向きが Left（Front 以外）は Err。
#[test]
fn from_json_rejects_dragged_not_facing_front() {
    let mut rows = json_rows();
    row_mut(&mut rows, "grabbed").from = ["dragged", "left"];
    row_mut(&mut rows, "grabbed").to = ["dragged", "left"];
    assert!(ClipSet::from_json(&rows_json(8, &rows)).is_err());
}

// from_json: Attention の向きが Right（Front 以外）は Err。
#[test]
fn from_json_rejects_attention_not_facing_front() {
    let mut rows = json_rows();
    row_mut(&mut rows, "call").to = ["attention", "right"];
    assert!(ClipSet::from_json(&rows_json(8, &rows)).is_err());
}

// from_json: JSON として壊れた入力は Err（panic しない）。
#[test]
fn from_json_rejects_malformed_text() {
    assert!(ClipSet::from_json("これは JSON ではない").is_err());
    assert!(ClipSet::from_json("").is_err());
}
