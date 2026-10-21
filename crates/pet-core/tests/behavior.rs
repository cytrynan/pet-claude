//! `pet_core::behavior` の統合テスト（テスト先行体制: 本番コードより先に書いた検収基準）。
//!
//! 原本: `.steering/20261021-phase1-behavior/requirements.md`
//! 公開 API（`pet_core::behavior::*` と `pet_core::sprite::Pose`）だけを使う。
//! 乱数には依存しない: 散歩・休みの長さは min==max で固定し、向きは RunRight / RunLeft の
//! どちらも許容して pose と vx の符号の整合だけを検査する（要件 5「乱数」の項）。

use pet_core::behavior::{AgentEvent, Behavior, BehaviorConfig, Output, UserEvent};
use pet_core::sprite::Pose;

/// テストで使う歩く速さ。
const SPEED: f32 = 40.0;
/// 反応の長さ（要件 1-2 の値。`native_clip` から求めた値と一致することもテストで確かめる）。
const JUMP_MS: u64 = 1680;
const FAIL_MS: u64 = 2440;

/// 散歩 500ms・休み 300ms を固定した設定。残りの 3 つだけを変える。
fn config(walk_after_ms: u64, sleep_after_ms: u64, agent_timeout_ms: u64) -> BehaviorConfig {
    BehaviorConfig {
        walk_after_ms,
        sleep_after_ms,
        walk_min_ms: 500,
        walk_max_ms: 500,
        rest_min_ms: 300,
        rest_max_ms: 300,
        walk_speed: SPEED,
        agent_timeout_ms,
    }
}

/// 自律行動を短い時計で見る設定: 散歩開始 1000ms・睡眠 5000ms・タイムアウト 1000ms。
fn cfg() -> BehaviorConfig {
    config(1000, 5000, 1000)
}

/// 自律行動が割り込まない設定（反応・セッションだけを見るとき）。
fn quiet() -> BehaviorConfig {
    config(1_000_000, 2_000_000, 1_000_000)
}

fn new_at(now_ms: u64) -> Behavior {
    Behavior::new(cfg(), 7, now_ms)
}

fn quiet_at(now_ms: u64) -> Behavior {
    Behavior::new(quiet(), 7, now_ms)
}

fn session(name: &str) -> String {
    name.to_string()
}

/// `native_clip()` の `cycle_ms × loops`。
fn reaction_len(pose: Pose) -> u64 {
    let clip = pose.native_clip();
    u64::from(clip.cycle_ms) * u64::from(clip.loops.expect("反応のポーズは有限ループ"))
}

/// 止まっているポーズ（vx=0）であること。
fn assert_still(out: Output, pose: Pose) {
    assert_eq!(out.pose, pose, "pose が違う: {out:?}");
    assert_eq!(out.vx, 0.0, "止まっているポーズなら vx は 0.0: {out:?}");
}

/// 散歩中であること（向きはどちらでもよい）。pose と vx の符号が整合することを検査し、向きを返す。
fn assert_walking(out: Output, speed: f32) -> Pose {
    match out.pose {
        Pose::RunRight => assert_eq!(out.vx, speed, "RunRight なら vx=+walk_speed: {out:?}"),
        Pose::RunLeft => assert_eq!(out.vx, -speed, "RunLeft なら vx=-walk_speed: {out:?}"),
        other => panic!("散歩中のはずが {other:?} (vx={})", out.vx),
    }
    out.pose
}

fn opposite(dir: Pose) -> Pose {
    match dir {
        Pose::RunRight => Pose::RunLeft,
        Pose::RunLeft => Pose::RunRight,
        other => panic!("向きではない: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 既定値・識別子契約
// ---------------------------------------------------------------------------

// 要件 既定値: Default の各値が表のとおり
#[test]
fn default_config_matches_spec_table() {
    let d = BehaviorConfig::default();
    assert_eq!(d.walk_after_ms, 90_000);
    assert_eq!(d.sleep_after_ms, 300_000);
    assert_eq!(d.walk_min_ms, 3_000);
    assert_eq!(d.walk_max_ms, 8_000);
    assert_eq!(d.rest_min_ms, 10_000);
    assert_eq!(d.rest_max_ms, 30_000);
    assert_eq!(d.walk_speed, 40.0);
    assert_eq!(d.agent_timeout_ms, 120_000);
}

// 要件 識別子契約: 派生（Clone/Copy/PartialEq/Eq/Debug）と Output の形、反応の長さの定数
#[test]
fn identifier_contract_types_derive_as_specified() {
    let config = BehaviorConfig::default();
    let config_clone = config.clone();
    assert_eq!(config, config_clone);

    let user = UserEvent::Click;
    let user_copy = user; // Copy
    assert_eq!(user, user_copy);
    assert_ne!(UserEvent::DragStart, UserEvent::DragEnd);

    let agent = AgentEvent::Prompt {
        session: session("a"),
    };
    assert_eq!(agent.clone(), agent);
    assert_ne!(
        agent,
        AgentEvent::ToolStart {
            session: session("a")
        }
    );

    let mut b = Behavior::new(config_clone, 0, 0);
    let out = b.tick(0);
    let out_copy = out; // Copy
    assert_eq!(out, out_copy);
    assert_eq!(
        out,
        Output {
            pose: Pose::Idle,
            vx: 0.0
        }
    );

    let text = format!("{config:?} {user:?} {agent:?} {out:?}");
    assert!(text.contains("Idle"));

    // 要件 1-2: 反応の長さは native_clip の cycle_ms × loops
    assert_eq!(reaction_len(Pose::Jumping), JUMP_MS);
    assert_eq!(reaction_len(Pose::Failed), FAIL_MS);
}

// 要件 既定値 / 5 / 4: Default のまま 90s で散歩、300s で睡眠（散歩は最短 3s なので 93s 直前はまだ散歩中）
#[test]
fn default_config_walks_at_90s_and_sleeps_at_300s() {
    let mut b = Behavior::new(BehaviorConfig::default(), 1, 0);
    assert_still(b.tick(89_999), Pose::Idle);
    assert_walking(b.tick(90_000), 40.0);
    assert_walking(b.tick(92_999), 40.0);
    assert_ne!(b.tick(299_999).pose, Pose::Sleep);
    assert_still(b.tick(300_000), Pose::Sleep);
}

// ---------------------------------------------------------------------------
// 6. 待機
// ---------------------------------------------------------------------------

// 要件 6: 上のどれでもなければ Idle・vx=0（new 直後と walk_after 未満）
#[test]
fn idle_before_walk_after() {
    let mut b = new_at(0);
    assert_still(b.tick(0), Pose::Idle);
    assert_still(b.tick(999), Pose::Idle);

    // new の now_ms から数える
    let mut b = new_at(5000);
    assert_still(b.tick(5000), Pose::Idle);
    assert_still(b.tick(5999), Pose::Idle);
    assert_walking(b.tick(6000), SPEED);
}

// ---------------------------------------------------------------------------
// 1. 優先順位
// ---------------------------------------------------------------------------

// 要件 1-1: DragStart から DragEnd までは Grabbed・vx=0
#[test]
fn grabbed_between_drag_start_and_drag_end() {
    let mut b = quiet_at(0);
    b.on_user(UserEvent::DragStart, 10);
    assert_still(b.tick(10), Pose::Grabbed);
    assert_still(b.tick(500), Pose::Grabbed);
    b.on_user(UserEvent::DragEnd, 600);
    assert_still(b.tick(600), Pose::Idle);
}

// 要件 1-1 / 3: つままれ中は無操作 0（眠らない）。DragEnd の now_ms から数え直す
#[test]
fn grabbed_elapsed_is_zero_and_drag_end_restarts_clock() {
    let mut b = new_at(0);
    b.on_user(UserEvent::DragStart, 0);
    assert_still(b.tick(10_000), Pose::Grabbed);
    b.on_user(UserEvent::DragEnd, 10_000);
    assert_still(b.tick(10_000), Pose::Idle);
    assert_still(b.tick(10_999), Pose::Idle);
    assert_walking(b.tick(11_000), SPEED);
}

// 要件 1-2: クリック → Jumping が 1680ms（1ms 前は残り、ちょうどで消える）
#[test]
fn click_plays_jumping_for_exact_clip_length() {
    let mut b = quiet_at(0);
    b.on_user(UserEvent::Click, 100);
    assert_still(b.tick(100), Pose::Jumping);
    assert_still(b.tick(100 + JUMP_MS - 1), Pose::Jumping);
    assert_still(b.tick(100 + JUMP_MS), Pose::Idle);
}

// 要件 1-2 / 2: Done → Jumping 1680ms、Error → Failed 2440ms
#[test]
fn done_plays_jumping_and_error_plays_failed() {
    let mut b = quiet_at(0);
    b.on_agent(
        AgentEvent::Done {
            session: session("a"),
        },
        0,
    );
    assert_still(b.tick(0), Pose::Jumping);
    assert_still(b.tick(JUMP_MS - 1), Pose::Jumping);
    assert_still(b.tick(JUMP_MS), Pose::Idle);

    let mut b = quiet_at(0);
    b.on_agent(
        AgentEvent::Error {
            session: session("a"),
        },
        50,
    );
    assert_still(b.tick(50), Pose::Failed);
    assert_still(b.tick(50 + FAIL_MS - 1), Pose::Failed);
    assert_still(b.tick(50 + FAIL_MS), Pose::Idle);
}

// 要件 1-2 / 壊す: 反応中に別の反応が来たら新しい反応で置き換え、開始時刻も新しくなる
#[test]
fn new_reaction_replaces_running_one_and_restarts_clock() {
    // Jumping → Failed
    let mut b = quiet_at(0);
    b.on_user(UserEvent::Click, 0);
    assert_still(b.tick(500), Pose::Jumping);
    b.on_agent(
        AgentEvent::Error {
            session: session("x"),
        },
        1000,
    );
    assert_still(b.tick(1000), Pose::Failed);
    assert_still(b.tick(1000 + FAIL_MS - 1), Pose::Failed);
    assert_still(b.tick(1000 + FAIL_MS), Pose::Idle);

    // Failed → Jumping（長い反応が短い反応で置き換わる）
    let mut b = quiet_at(0);
    b.on_agent(
        AgentEvent::Error {
            session: session("x"),
        },
        0,
    );
    b.on_user(UserEvent::Click, 100);
    assert_still(b.tick(100), Pose::Jumping);
    assert_still(b.tick(100 + JUMP_MS), Pose::Idle); // 元の Failed（〜2440）は残らない

    // Jumping → Jumping（同じ反応でも開始時刻が更新される。questions.md 1 を参照）
    let mut b = quiet_at(0);
    b.on_user(UserEvent::Click, 0);
    b.on_user(UserEvent::Click, 1000);
    assert_still(b.tick(JUMP_MS), Pose::Jumping); // 最初の反応だけなら消えている時刻
    assert_still(b.tick(1000 + JUMP_MS - 1), Pose::Jumping);
    assert_still(b.tick(1000 + JUMP_MS), Pose::Idle);
}

// 要件 1-2: つままれに隠れているあいだも反応の残り時間は減る。DragStart / DragEnd は反応を消さない
#[test]
fn reaction_keeps_running_while_grabbed_and_drag_does_not_clear_it() {
    // DragEnd が反応の終わりの 1ms 前 → まだ Jumping
    let mut b = quiet_at(0);
    b.on_user(UserEvent::Click, 0);
    b.on_user(UserEvent::DragStart, 0);
    assert_still(b.tick(0), Pose::Grabbed);
    assert_still(b.tick(1000), Pose::Grabbed);
    b.on_user(UserEvent::DragEnd, JUMP_MS - 1);
    assert_still(b.tick(JUMP_MS - 1), Pose::Jumping);
    assert_still(b.tick(JUMP_MS), Pose::Idle);

    // DragEnd がちょうど反応の長さ → 反応はもう返らない
    let mut b = quiet_at(0);
    b.on_user(UserEvent::Click, 0);
    b.on_user(UserEvent::DragStart, 0);
    b.on_user(UserEvent::DragEnd, JUMP_MS);
    assert_still(b.tick(JUMP_MS), Pose::Idle);
}

// 要件 1 / 壊す: つままれ中の Agent イベント（Done）は Grabbed に隠れ、離すと残りの反応が見える
#[test]
fn agent_event_while_grabbed_is_hidden_until_drag_end() {
    let mut b = quiet_at(0);
    b.on_user(UserEvent::DragStart, 0);
    b.on_agent(
        AgentEvent::Done {
            session: session("a"),
        },
        100,
    );
    assert_still(b.tick(100), Pose::Grabbed);
    assert_still(b.tick(900), Pose::Grabbed);
    b.on_user(UserEvent::DragEnd, 1000);
    assert_still(b.tick(1000), Pose::Jumping);
    assert_still(b.tick(100 + JUMP_MS - 1), Pose::Jumping);
    assert_still(b.tick(100 + JUMP_MS), Pose::Idle);
}

// 要件 1 / 壊す: つままれ中の Prompt もセッションを作る（離したら Running）
#[test]
fn prompt_while_grabbed_registers_session_and_shows_after_drag_end() {
    let mut b = quiet_at(0);
    b.on_user(UserEvent::DragStart, 0);
    b.on_agent(
        AgentEvent::Prompt {
            session: session("a"),
        },
        100,
    );
    assert_still(b.tick(100), Pose::Grabbed);
    assert_still(b.tick(500), Pose::Grabbed);
    b.on_user(UserEvent::DragEnd, 600);
    assert_still(b.tick(600), Pose::Running);
}

// 要件 1: 優先順位 つままれ > 反応 > Claude の状態 > 自律行動
#[test]
fn priority_grabbed_over_reaction_over_agent_state() {
    let mut b = quiet_at(0);
    b.on_agent(
        AgentEvent::Prompt {
            session: session("a"),
        },
        0,
    );
    assert_still(b.tick(0), Pose::Running);
    b.on_user(UserEvent::Click, 0);
    assert_still(b.tick(0), Pose::Jumping);
    b.on_user(UserEvent::DragStart, 0);
    assert_still(b.tick(0), Pose::Grabbed);
    b.on_user(UserEvent::DragEnd, 10);
    assert_still(b.tick(10), Pose::Jumping);
    assert_still(b.tick(JUMP_MS), Pose::Running); // 反応が終わると Claude の状態
}

// 要件 1-3: 操作待ちが作業中より優先。同じセッションの状態は最後のイベントで上書き
#[test]
fn waiting_beats_running_and_latest_event_wins() {
    let mut b = quiet_at(0);
    b.on_agent(
        AgentEvent::Prompt {
            session: session("a"),
        },
        0,
    );
    assert_still(b.tick(0), Pose::Running);
    b.on_agent(
        AgentEvent::NeedsInput {
            session: session("b"),
        },
        10,
    );
    assert_still(b.tick(10), Pose::Waiting);
    b.on_agent(
        AgentEvent::ToolStart {
            session: session("b"),
        },
        20,
    );
    assert_still(b.tick(20), Pose::Running);
    b.on_agent(
        AgentEvent::NeedsInput {
            session: session("a"),
        },
        30,
    );
    assert_still(b.tick(30), Pose::Waiting);
}

// 要件 1-4: held 中は自律行動が常に待機（散歩も睡眠もしない）。1〜3 は held の影響を受けない
#[test]
fn held_forces_idle_for_autonomous_but_not_for_higher_priorities() {
    let mut b = Behavior::new(config(1000, 5000, 1_000_000), 7, 0);
    b.set_held(true, 0);
    assert_still(b.tick(1000), Pose::Idle);
    assert_still(b.tick(5000), Pose::Idle);
    assert_still(b.tick(100_000), Pose::Idle);
    b.on_agent(
        AgentEvent::Prompt {
            session: session("a"),
        },
        100_000,
    );
    assert_still(b.tick(100_000), Pose::Running);
    b.on_user(UserEvent::Click, 100_000);
    assert_still(b.tick(100_000), Pose::Jumping);
    b.on_user(UserEvent::DragStart, 100_000);
    assert_still(b.tick(100_000), Pose::Grabbed);
}

// 要件 1-4 / 壊す: held 中の Error は反応として見え、終わると held の待機に戻る。held を外すと時計が動く
#[test]
fn error_while_held_plays_failed_then_returns_to_idle() {
    let mut b = new_at(0);
    b.set_held(true, 0);
    b.on_agent(
        AgentEvent::Error {
            session: session("a"),
        },
        100,
    );
    assert_still(b.tick(100), Pose::Failed);
    assert_still(b.tick(100 + FAIL_MS - 1), Pose::Failed);
    assert_still(b.tick(100 + FAIL_MS), Pose::Idle);
    assert_still(b.tick(100_000), Pose::Idle); // held のまま＝眠らない
    b.set_held(false, 100_000);
    assert_still(b.tick(100_999), Pose::Idle);
    assert_walking(b.tick(101_000), SPEED);
    assert_still(b.tick(105_000), Pose::Sleep);
}

// ---------------------------------------------------------------------------
// 2. Claude のセッション状態
// ---------------------------------------------------------------------------

// 要件 2: Prompt / ToolStart は作業中。Done はそのセッションだけ消す（他のセッションは残る）
#[test]
fn done_clears_only_its_session() {
    let mut b = quiet_at(0);
    b.on_agent(
        AgentEvent::Prompt {
            session: session("a"),
        },
        0,
    );
    b.on_agent(
        AgentEvent::ToolStart {
            session: session("b"),
        },
        0,
    );
    assert_still(b.tick(0), Pose::Running);
    b.on_agent(
        AgentEvent::Done {
            session: session("a"),
        },
        0,
    );
    assert_still(b.tick(0), Pose::Jumping);
    assert_still(b.tick(JUMP_MS), Pose::Running); // b が残っている
    b.on_agent(
        AgentEvent::Done {
            session: session("b"),
        },
        JUMP_MS,
    );
    assert_still(b.tick(JUMP_MS), Pose::Jumping);
    assert_still(b.tick(JUMP_MS * 2), Pose::Idle); // セッションが無い
}

// 要件 2: Error は操作待ちのセッションも消す
#[test]
fn error_clears_waiting_session() {
    let mut b = quiet_at(0);
    b.on_agent(
        AgentEvent::NeedsInput {
            session: session("a"),
        },
        0,
    );
    assert_still(b.tick(0), Pose::Waiting);
    b.on_agent(
        AgentEvent::Error {
            session: session("a"),
        },
        10,
    );
    assert_still(b.tick(10), Pose::Failed);
    assert_still(b.tick(10 + FAIL_MS), Pose::Idle);
}

// 要件 2: 知らないセッションの Done / Error も反応を起こす（状態は残らない）
#[test]
fn unknown_session_done_and_error_still_react() {
    let mut b = quiet_at(0);
    b.on_agent(
        AgentEvent::Done {
            session: session("ghost"),
        },
        0,
    );
    assert_still(b.tick(0), Pose::Jumping);
    assert_still(b.tick(JUMP_MS), Pose::Idle);
    b.on_agent(
        AgentEvent::Error {
            session: session("ghost"),
        },
        2000,
    );
    assert_still(b.tick(2000), Pose::Failed);
    assert_still(b.tick(2000 + FAIL_MS), Pose::Idle);
}

// 要件 2: タイムアウトはちょうど等しい時刻でも捨てる。そのセッションの更新で延びる
#[test]
fn session_times_out_at_exact_boundary_and_updates_extend_it() {
    let c = config(1_000_000, 2_000_000, 1000);
    let mut b = Behavior::new(c.clone(), 7, 0);
    b.on_agent(
        AgentEvent::Prompt {
            session: session("a"),
        },
        0,
    );
    assert_still(b.tick(999), Pose::Running);
    assert_still(b.tick(1000), Pose::Idle);

    let mut b = Behavior::new(c, 7, 0);
    b.on_agent(
        AgentEvent::NeedsInput {
            session: session("a"),
        },
        0,
    );
    b.on_agent(
        AgentEvent::ToolStart {
            session: session("a"),
        },
        600,
    );
    assert_still(b.tick(1000), Pose::Running);
    assert_still(b.tick(1599), Pose::Running);
    assert_still(b.tick(1600), Pose::Idle);
}

// 要件 2: セッションごとに独立にタイムアウトする
#[test]
fn sessions_time_out_independently() {
    let mut b = Behavior::new(config(1_000_000, 2_000_000, 1000), 7, 0);
    b.on_agent(
        AgentEvent::NeedsInput {
            session: session("a"),
        },
        0,
    );
    b.on_agent(
        AgentEvent::Prompt {
            session: session("b"),
        },
        500,
    );
    assert_still(b.tick(999), Pose::Waiting);
    assert_still(b.tick(1000), Pose::Running); // a が消え b が残る
    assert_still(b.tick(1499), Pose::Running);
    assert_still(b.tick(1500), Pose::Idle);
}

// ---------------------------------------------------------------------------
// 3. 無操作の時計
// ---------------------------------------------------------------------------

// 要件 3: 有効なセッションがあるあいだは無操作 0（sleep_after を超えても眠らない）。切れた後は数え直し
#[test]
fn active_session_keeps_idle_clock_at_zero() {
    let mut b = Behavior::new(config(1000, 5000, 100_000), 7, 0);
    b.on_agent(
        AgentEvent::ToolStart {
            session: session("a"),
        },
        0,
    );
    assert_still(b.tick(50_000), Pose::Running);
    assert_still(b.tick(100_000), Pose::Idle); // 捨てた時点で最後の操作 = 0 + 100_000
    assert_still(b.tick(100_999), Pose::Idle);
    assert_walking(b.tick(101_000), SPEED);
}

// 要件 3 / 壊す: タイムアウトで消えたら「最後の操作」は max(最後の操作, 最後の更新 + timeout) に数え直す
#[test]
fn timeout_recounts_idle_clock_from_last_update_plus_timeout() {
    let mut b = new_at(0);
    b.on_agent(
        AgentEvent::Prompt {
            session: session("a"),
        },
        0,
    );
    // 1000 で捨てられ、最後の操作は max(0, 0 + 1000) = 1000
    assert_still(b.tick(1000), Pose::Idle);
    assert_still(b.tick(1999), Pose::Idle);
    assert_walking(b.tick(2000), SPEED);
    // 睡眠の境界も 1000 起点（0 起点のままなら 5999 で眠ってしまう）
    assert_ne!(b.tick(5999).pose, Pose::Sleep);
    assert_still(b.tick(6000), Pose::Sleep);
}

// 要件 3 / 壊す: 捨てる tick が遅れて来ても、数え直しは「更新 + timeout」から（tick の時刻からではない）
#[test]
fn late_tick_after_timeout_counts_from_update_plus_timeout_not_tick_time() {
    let mut b = new_at(0);
    b.on_agent(
        AgentEvent::Prompt {
            session: session("a"),
        },
        0,
    );
    // 次の tick が 3000: 捨てた時点で最後の操作 = 1000、無操作 2000 → この tick で散歩開始（遡らない）
    assert_walking(b.tick(3000), SPEED);
    assert_walking(b.tick(3499), SPEED);
    assert_still(b.tick(3500), Pose::Idle);
    assert_ne!(b.tick(5999).pose, Pose::Sleep);
    assert_still(b.tick(6000), Pose::Sleep);
}

// 要件 3 / 壊す: max() — タイムアウトより後の操作があればそちらが残る
#[test]
fn timeout_recount_keeps_later_operation() {
    let mut b = new_at(0);
    b.on_agent(
        AgentEvent::Prompt {
            session: session("a"),
        },
        0,
    );
    b.set_held(true, 700);
    assert_still(b.tick(900), Pose::Running); // held は Claude の状態に影響しない
    b.set_held(false, 1500); // 最後の操作 = 1500
    // この tick でセッションが捨てられる: max(1500, 0 + 1000) = 1500
    assert_still(b.tick(1500), Pose::Idle);
    assert_still(b.tick(2499), Pose::Idle);
    assert_walking(b.tick(2500), SPEED);
}

// 要件 3: set_held は値が変わったときだけ「最後の操作」を更新する（同じ値は何も起きない。初期値は false）
#[test]
fn set_held_same_value_does_not_touch_idle_clock() {
    let mut b = new_at(0);
    b.set_held(false, 900); // 初期値 false → 変化なし
    assert_walking(b.tick(1000), SPEED);

    let mut b = new_at(0);
    b.set_held(true, 100);
    b.set_held(true, 800); // 変化なし
    b.set_held(false, 900); // 変化あり → 最後の操作 = 900
    assert_still(b.tick(1899), Pose::Idle);
    assert_walking(b.tick(1900), SPEED);
}

// 要件 3: on_agent（知らないセッションの Done）も「最後の操作」を更新する
#[test]
fn agent_event_updates_idle_clock() {
    let mut b = Behavior::new(config(1000, 2000, 1000), 7, 0);
    b.on_agent(
        AgentEvent::Done {
            session: session("ghost"),
        },
        500,
    );
    // 反応は 500..2180。2180 で: 最後の操作が 500 なら無操作 1680 → 散歩、0 のままなら 2180 → 睡眠
    assert_still(b.tick(500 + JUMP_MS - 1), Pose::Jumping);
    assert_walking(b.tick(500 + JUMP_MS), SPEED);
}

// 要件 3: Click も「最後の操作」を更新する（反応後は Click の時刻から数える）
#[test]
fn click_updates_idle_clock() {
    let mut b = Behavior::new(config(1000, 2000, 1000), 7, 0);
    b.on_user(UserEvent::Click, 500);
    assert_still(b.tick(500 + JUMP_MS - 1), Pose::Jumping);
    assert_walking(b.tick(500 + JUMP_MS), SPEED);
}

// 要件 3: on_blocked は「最後の操作」を更新しない
#[test]
fn on_blocked_does_not_update_idle_clock() {
    let mut b = new_at(0);
    b.on_blocked(900);
    assert_walking(b.tick(1000), SPEED);
}

// ---------------------------------------------------------------------------
// 4. 睡眠
// ---------------------------------------------------------------------------

// 要件 4: 無操作 sleep_after_ms ちょうどで Sleep・vx=0（1ms 前は眠らない）
#[test]
fn sleeps_at_exact_sleep_after() {
    let mut b = new_at(0);
    assert_ne!(b.tick(4999).pose, Pose::Sleep);
    assert_still(b.tick(5000), Pose::Sleep);
    assert_still(b.tick(60_000), Pose::Sleep);
}

// 要件 4: 散歩中でも眠る
#[test]
fn sleeps_even_in_the_middle_of_a_walk() {
    let c = BehaviorConfig {
        walk_min_ms: 10_000,
        walk_max_ms: 10_000,
        ..cfg()
    };
    let mut b = Behavior::new(c, 7, 0);
    assert_walking(b.tick(1000), SPEED);
    assert_walking(b.tick(4999), SPEED);
    assert_still(b.tick(5000), Pose::Sleep);
}

// 要件 4: 操作で起きる（set_held の変化・DragStart/DragEnd・Prompt）
#[test]
fn wakes_on_any_operation() {
    let mut b = new_at(0);
    assert_still(b.tick(5000), Pose::Sleep);
    b.set_held(true, 6000);
    assert_still(b.tick(6000), Pose::Idle);
    b.set_held(false, 6000);
    assert_still(b.tick(6000), Pose::Idle);
    assert_still(b.tick(6999), Pose::Idle);
    assert_walking(b.tick(7000), SPEED);

    let mut b = new_at(0);
    assert_still(b.tick(5000), Pose::Sleep);
    b.on_user(UserEvent::DragStart, 6000);
    assert_still(b.tick(6000), Pose::Grabbed);
    b.on_user(UserEvent::DragEnd, 6100);
    assert_still(b.tick(6100), Pose::Idle);
    assert_still(b.tick(7099), Pose::Idle);
    assert_walking(b.tick(7100), SPEED);

    let mut b = Behavior::new(config(1000, 5000, 1_000_000), 7, 0);
    assert_still(b.tick(5000), Pose::Sleep);
    b.on_agent(
        AgentEvent::Prompt {
            session: session("a"),
        },
        6000,
    );
    assert_still(b.tick(6000), Pose::Running);
}

// ---------------------------------------------------------------------------
// 5. 散歩
// ---------------------------------------------------------------------------

// 要件 5: walk_after_ms ちょうどで散歩開始、1ms 前は待機。pose と vx の符号が整合
#[test]
fn walk_starts_at_exact_walk_after_with_consistent_direction() {
    let mut b = new_at(0);
    assert_still(b.tick(999), Pose::Idle);
    assert_walking(b.tick(1000), SPEED);
}

// 要件 5: 向きは seed によらず pose と vx が整合する（どちらの向きでもよい）
#[test]
fn walk_direction_is_consistent_for_any_seed() {
    for seed in 0..64u64 {
        let mut b = Behavior::new(cfg(), seed, 0);
        assert_walking(b.tick(1000), SPEED);
    }
}

// 要件 5: 散歩 → 休み → 散歩 を固定長で交互に。切り替えはちょうどの時刻、1ms 前は前の状態
#[test]
fn alternates_walk_and_rest_with_fixed_lengths() {
    let mut b = new_at(0);
    let dir = assert_walking(b.tick(1000), SPEED);
    assert_eq!(
        assert_walking(b.tick(1499), SPEED),
        dir,
        "散歩中は向きが変わらない"
    );
    assert_still(b.tick(1500), Pose::Idle); // 休み開始
    assert_still(b.tick(1799), Pose::Idle);
    assert_walking(b.tick(1800), SPEED); // 2 回目の散歩
    assert_walking(b.tick(2299), SPEED);
    assert_still(b.tick(2300), Pose::Idle);
    assert_walking(b.tick(2600), SPEED);
}

// 要件 5: 散歩中の vx は設定の walk_speed
#[test]
fn walk_vx_uses_configured_speed() {
    let c = BehaviorConfig {
        walk_speed: 12.5,
        ..cfg()
    };
    let mut b = Behavior::new(c, 7, 0);
    assert_walking(b.tick(1000), 12.5);
}

// 要件 5: 散歩・休みの開始時刻は条件を最初に満たした tick の now_ms（遡らない）
#[test]
fn state_start_time_is_the_tick_that_observed_it_not_backdated() {
    let mut b = new_at(0);
    // 散歩に入れる最初の tick が 1700 → 散歩開始 1700（1000 に遡らない）
    assert_walking(b.tick(1700), SPEED);
    assert_walking(b.tick(2199), SPEED);
    assert_still(b.tick(2200), Pose::Idle); // 休み開始 2200（〜2500）
    // 休みが終わっているのを 2900 で初めて観測 → 散歩開始 2900（2500 に遡らない）
    assert_walking(b.tick(2900), SPEED);
    assert_walking(b.tick(3399), SPEED);
    assert_still(b.tick(3400), Pose::Idle);
}

// 要件 5: 1 回の tick で切り替えは高々 1 段（散歩と休みをまとめて飛ばさない）
#[test]
fn at_most_one_transition_per_tick() {
    let mut b = new_at(0);
    assert_walking(b.tick(1000), SPEED);
    // 散歩 500 + 休み 300 を超えて 3000 まで飛んでも、休みに 1 段だけ進む
    assert_still(b.tick(3000), Pose::Idle);
    assert_still(b.tick(3299), Pose::Idle);
    assert_walking(b.tick(3300), SPEED);
}

// 要件 5 / 壊す: 同じ now_ms で tick を繰り返しても 2 回目以降は状態を進めない
#[test]
fn repeated_ticks_at_same_now_do_not_advance_state() {
    let mut b = new_at(0);
    let first = b.tick(1000);
    assert_walking(first, SPEED);
    assert_eq!(b.tick(1000), first);
    assert_eq!(b.tick(1000), first);
    assert_still(b.tick(1500), Pose::Idle);
    assert_still(b.tick(1500), Pose::Idle);
    assert_walking(b.tick(1800), SPEED);

    // 休みに入った直後、同じ now_ms を繰り返しても（休みの長さを超えていても）散歩に進まない
    let mut b = new_at(0);
    assert_walking(b.tick(1000), SPEED);
    assert_still(b.tick(3000), Pose::Idle);
    assert_still(b.tick(3000), Pose::Idle);
    assert_still(b.tick(3000), Pose::Idle);
    assert_still(b.tick(3299), Pose::Idle);
    assert_walking(b.tick(3300), SPEED);
}

// 要件 5 / 壊す: 同じ now_ms の繰り返しは乱数も引かない（繰り返し有り / 無しで出力列が一致する）
#[test]
fn repeated_ticks_do_not_consume_randomness() {
    let c = BehaviorConfig {
        walk_min_ms: 300,
        walk_max_ms: 900,
        rest_min_ms: 100,
        rest_max_ms: 400,
        ..cfg()
    };
    let mut plain = Behavior::new(c.clone(), 99, 0);
    let mut noisy = Behavior::new(c, 99, 0);
    for i in 0..60u64 {
        let t = 1000 + i * 50;
        let a = plain.tick(t);
        let b1 = noisy.tick(t);
        let b2 = noisy.tick(t);
        let b3 = noisy.tick(t);
        assert_eq!(a, b1, "t={t}");
        assert_eq!(b1, b2, "t={t}");
        assert_eq!(b2, b3, "t={t}");
    }
}

// 要件 5 / 壊す: on_blocked は散歩中なら向きを反転し、残り時間はそのまま。同じ now_ms でも次の tick は新しい状態
#[test]
fn on_blocked_reverses_direction_and_keeps_remaining_walk_time() {
    let mut b = new_at(0);
    let dir = assert_walking(b.tick(1000), SPEED);
    assert_eq!(assert_walking(b.tick(1200), SPEED), dir);
    b.on_blocked(1200);
    let flipped = assert_walking(b.tick(1200), SPEED);
    assert_eq!(flipped, opposite(dir));
    assert_eq!(assert_walking(b.tick(1499), SPEED), flipped);
    assert_still(b.tick(1500), Pose::Idle); // 残り時間は変わらない

    // 2 回反転すると元の向き
    let mut b = new_at(0);
    let dir = assert_walking(b.tick(1000), SPEED);
    b.on_blocked(1100);
    b.on_blocked(1100);
    assert_eq!(assert_walking(b.tick(1100), SPEED), dir);
}

// 要件 5 / 壊す: 散歩中でなければ on_blocked は何もしない（休み・散歩前の待機・睡眠）
#[test]
fn on_blocked_outside_walk_does_nothing() {
    // 休み中
    let mut b = new_at(0);
    assert_walking(b.tick(1000), SPEED);
    assert_still(b.tick(1500), Pose::Idle);
    b.on_blocked(1600);
    assert_still(b.tick(1600), Pose::Idle);
    assert_still(b.tick(1799), Pose::Idle);
    assert_walking(b.tick(1800), SPEED);

    // 散歩前の待機（時計も動かさない）
    let mut b = new_at(0);
    b.on_blocked(500);
    assert_still(b.tick(500), Pose::Idle);
    assert_walking(b.tick(1000), SPEED);

    // 睡眠中
    let mut b = new_at(0);
    assert_still(b.tick(5000), Pose::Sleep);
    b.on_blocked(5100);
    assert_still(b.tick(5100), Pose::Sleep);
}

// 要件 5: 操作で時計が戻ったら散歩も休みも打ち切り、次に入れるようになったら散歩から始める
#[test]
fn operation_cancels_walk_or_rest_and_next_cycle_starts_with_walk() {
    // 散歩中に held → 外す
    let mut b = new_at(0);
    assert_walking(b.tick(1000), SPEED);
    b.set_held(true, 1200);
    assert_still(b.tick(1200), Pose::Idle);
    b.set_held(false, 1300); // 最後の操作 = 1300
    assert_still(b.tick(1300), Pose::Idle);
    assert_still(b.tick(2299), Pose::Idle);
    assert_walking(b.tick(2300), SPEED); // 新しい散歩（残り時間を引き継がない: 〜2800）
    assert_walking(b.tick(2799), SPEED);
    assert_still(b.tick(2800), Pose::Idle);

    // 休み中に DragStart / DragEnd
    let mut b = new_at(0);
    assert_walking(b.tick(1000), SPEED);
    assert_still(b.tick(1500), Pose::Idle); // 休み（〜1800）
    b.on_user(UserEvent::DragStart, 1600);
    b.on_user(UserEvent::DragEnd, 1600); // 最後の操作 = 1600
    assert_still(b.tick(1600), Pose::Idle);
    assert_still(b.tick(1800), Pose::Idle); // 元の休みの終わりでは散歩に戻らない
    assert_still(b.tick(2599), Pose::Idle);
    assert_walking(b.tick(2600), SPEED); // 休みの続きではなく散歩から
    assert_walking(b.tick(3099), SPEED);
    assert_still(b.tick(3100), Pose::Idle);

    // 散歩中に Click → 反応のあと Click の時刻から数え直して散歩から
    let mut b = new_at(0);
    assert_walking(b.tick(1000), SPEED);
    b.on_user(UserEvent::Click, 1200); // 最後の操作 = 1200
    assert_still(b.tick(1200), Pose::Jumping);
    assert_still(b.tick(1200 + JUMP_MS - 1), Pose::Jumping);
    assert_walking(b.tick(1200 + JUMP_MS), SPEED); // 無操作 1680 ≥ 1000 → 散歩開始（〜+500）
    assert_walking(b.tick(1200 + JUMP_MS + 499), SPEED);
    assert_still(b.tick(1200 + JUMP_MS + 500), Pose::Idle);
}

// 要件 5: 同じ seed・同じ入力列（tick の now_ms 列を含む）なら同じ Output 列（決定性）
#[test]
fn same_seed_and_inputs_give_identical_output_sequence() {
    let c = BehaviorConfig {
        walk_min_ms: 300,
        walk_max_ms: 900,
        rest_min_ms: 100,
        rest_max_ms: 400,
        sleep_after_ms: 50_000,
        agent_timeout_ms: 700,
        ..cfg()
    };
    let run = |seed: u64| -> Vec<Output> {
        let mut b = Behavior::new(c.clone(), seed, 0);
        let mut outs = Vec::new();
        for i in 0..600u64 {
            let t = i * 37;
            match i {
                55 => b.on_blocked(t),
                109 => b.on_user(UserEvent::Click, t),
                163 => b.on_agent(
                    AgentEvent::Prompt {
                        session: session("s"),
                    },
                    t,
                ),
                244 => b.set_held(true, t),
                270 => b.set_held(false, t),
                325 => b.on_user(UserEvent::DragStart, t),
                351 => b.on_user(UserEvent::DragEnd, t),
                406 => b.on_blocked(t),
                460 => b.on_agent(
                    AgentEvent::Done {
                        session: session("s"),
                    },
                    t,
                ),
                _ => {}
            }
            outs.push(b.tick(t));
            if i % 7 == 0 {
                outs.push(b.tick(t)); // 同じ now_ms の繰り返しも入力列の一部
            }
        }
        outs
    };
    let first = run(12_345);
    let second = run(12_345);
    assert_eq!(first, second);
    assert!(
        first
            .iter()
            .any(|o| matches!(o.pose, Pose::RunRight | Pose::RunLeft)),
        "散歩を含む列であること"
    );
    assert!(
        first.iter().any(|o| o.pose == Pose::Jumping),
        "反応を含む列であること"
    );
}
