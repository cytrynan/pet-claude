//! ペットの行動ロジック。いまどのポーズを取り、どちらへ歩くかを決める純粋な状態機械。
//!
//! 時刻・利用者の操作・Claude Code の出来事を入力にとり、`tick` のたびに
//! 「ポーズ」と「歩く速さ」を返す。OS には触らない。乱数は種を渡して決定的にする。
//! 仕様の正本は `.steering/20261021-phase1-behavior/requirements.md`。

use std::collections::BTreeMap;

use crate::sprite::Pose;

/// 調整できる定数。`Default` は仕様の既定値。
#[derive(Debug, Clone, PartialEq)]
pub struct BehaviorConfig {
    /// 無操作がこれだけ続いたら散歩を始めてよい。
    pub walk_after_ms: u64,
    /// 無操作がこれだけ続いたら眠る。
    pub sleep_after_ms: u64,
    /// 1 回の散歩の長さの下限。
    pub walk_min_ms: u64,
    /// 1 回の散歩の長さの上限。
    pub walk_max_ms: u64,
    /// 散歩と散歩のあいだの休み（待機）の下限。
    pub rest_min_ms: u64,
    /// 散歩と散歩のあいだの休みの上限。
    pub rest_max_ms: u64,
    /// 論理ピクセル毎秒（正の値）。
    pub walk_speed: f32,
    /// Claude の作業状態をこれだけ更新が無ければ捨てる。
    pub agent_timeout_ms: u64,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            walk_after_ms: 90_000,
            sleep_after_ms: 300_000,
            walk_min_ms: 3_000,
            walk_max_ms: 8_000,
            rest_min_ms: 10_000,
            rest_max_ms: 30_000,
            walk_speed: 40.0,
            agent_timeout_ms: 120_000,
        }
    }
}

/// 利用者がペットに対して行った操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserEvent {
    /// ペットがクリックされた（ドラッグではない）。
    Click,
    /// つまみ上げた。
    DragStart,
    /// 離した。
    DragEnd,
}

/// Claude Code 側の出来事。`session` で会話を区別する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    /// 利用者がプロンプトを送った＝作業を始めた。
    Prompt { session: String },
    /// ツールの実行を始めた。
    ToolStart { session: String },
    /// 承認・回答など利用者の操作を待っている。
    NeedsInput { session: String },
    /// 応答が終わった。
    Done { session: String },
    /// 失敗した。
    Error { session: String },
}

/// `tick` の戻り値。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Output {
    pub pose: Pose,
    /// 横方向の速さ（論理ピクセル毎秒）。正は右、負は左、歩いていない時は 0.0。
    pub vx: f32,
}

/// セッションの状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionKind {
    Working,
    Waiting,
}

#[derive(Debug, Clone, Copy)]
struct Session {
    kind: SessionKind,
    updated_ms: u64,
}

/// 一度だけ再生する反応（クリック・完了・失敗）。
#[derive(Debug, Clone, Copy)]
struct Reaction {
    pose: Pose,
    started_ms: u64,
}

impl Reaction {
    /// `native_clip` の `cycle_ms × loops`。
    fn len_ms(self) -> u64 {
        let clip = self.pose.native_clip();
        u64::from(clip.cycle_ms) * u64::from(clip.loops.unwrap_or(1))
    }

    fn is_active(self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.started_ms) < self.len_ms()
    }
}

/// 自律行動の散歩・休みの状態。開始時刻は状態に入ると決めた `tick` の `now_ms`。
#[derive(Debug, Clone, Copy)]
enum Auto {
    Walk {
        started_ms: u64,
        len_ms: u64,
        right: bool,
    },
    Rest {
        started_ms: u64,
        len_ms: u64,
    },
}

/// SplitMix64。`seed` だけに依存する自前の乱数。
#[derive(Debug, Clone)]
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `min..=max` から選ぶ（`max < min` のときは `min`）。
    fn range(&mut self, min: u64, max: u64) -> u64 {
        let span = max.saturating_sub(min).saturating_add(1);
        min + self.next_u64() % span
    }
}

/// ペットの行動を決める状態機械。
#[derive(Debug, Clone)]
pub struct Behavior {
    config: BehaviorConfig,
    rng: SplitMix64,
    /// 「最後の操作」の時刻。
    last_op_ms: u64,
    held: bool,
    grabbed: bool,
    reaction: Option<Reaction>,
    sessions: BTreeMap<String, Session>,
    auto: Option<Auto>,
    /// 散歩・休みを切り替えた `tick` の時刻。同じ時刻の再 `tick` で状態を進めないための印。
    advanced_at_ms: Option<u64>,
}

impl Behavior {
    /// `now_ms` を「最後の操作」の初期値にする。`held` とつままれ状態は false で始まる。
    pub fn new(config: BehaviorConfig, seed: u64, now_ms: u64) -> Self {
        Self {
            config,
            rng: SplitMix64(seed),
            last_op_ms: now_ms,
            held: false,
            grabbed: false,
            reaction: None,
            sessions: BTreeMap::new(),
            auto: None,
            advanced_at_ms: None,
        }
    }

    /// 利用者の操作を受ける。
    pub fn on_user(&mut self, event: UserEvent, now_ms: u64) {
        match event {
            UserEvent::Click => self.start_reaction(Pose::Jumping, now_ms),
            UserEvent::DragStart => self.grabbed = true,
            UserEvent::DragEnd => self.grabbed = false,
        }
        self.touch(now_ms);
    }

    /// Claude Code の出来事を受ける。
    pub fn on_agent(&mut self, event: AgentEvent, now_ms: u64) {
        match event {
            AgentEvent::Prompt { session } | AgentEvent::ToolStart { session } => {
                self.set_session(session, SessionKind::Working, now_ms);
            }
            AgentEvent::NeedsInput { session } => {
                self.set_session(session, SessionKind::Waiting, now_ms);
            }
            AgentEvent::Done { session } => {
                self.sessions.remove(&session);
                self.start_reaction(Pose::Jumping, now_ms);
            }
            AgentEvent::Error { session } => {
                self.sessions.remove(&session);
                self.start_reaction(Pose::Failed, now_ms);
            }
        }
        self.touch(now_ms);
    }

    /// バー・メニューが開いている間 true。値が変わったときだけ状態を更新する。
    pub fn set_held(&mut self, held: bool, now_ms: u64) {
        if self.held != held {
            self.held = held;
            self.touch(now_ms);
        }
    }

    /// 歩いた結果、作業領域の端に当たって動けなかったときに呼ぶ。散歩中なら向きを反転する。
    /// 「最後の操作」は更新しない。
    pub fn on_blocked(&mut self, _now_ms: u64) {
        if let Some(Auto::Walk { right, .. }) = &mut self.auto {
            *right = !*right;
        }
        self.advanced_at_ms = None;
    }

    /// 毎フレーム呼ぶ。`now_ms` は単調に増える（同じ値で何度呼んでもよい）。
    pub fn tick(&mut self, now_ms: u64) -> Output {
        self.expire_sessions(now_ms);

        if self.grabbed {
            return still(Pose::Grabbed);
        }
        if let Some(reaction) = self.reaction.filter(|r| r.is_active(now_ms)) {
            return still(reaction.pose);
        }
        if self
            .sessions
            .values()
            .any(|s| s.kind == SessionKind::Waiting)
        {
            return still(Pose::Waiting);
        }
        if !self.sessions.is_empty() {
            return still(Pose::Running);
        }
        self.autonomous(now_ms)
    }

    /// 「最後の操作」を更新し、散歩・休みを打ち切る。
    fn touch(&mut self, now_ms: u64) {
        self.last_op_ms = now_ms;
        self.auto = None;
        self.advanced_at_ms = None;
    }

    fn set_session(&mut self, session: String, kind: SessionKind, now_ms: u64) {
        self.sessions.insert(
            session,
            Session {
                kind,
                updated_ms: now_ms,
            },
        );
    }

    /// 反応を新しく始める（再生中なら置き換え、開始時刻も新しくする）。
    fn start_reaction(&mut self, pose: Pose, now_ms: u64) {
        self.reaction = Some(Reaction {
            pose,
            started_ms: now_ms,
        });
    }

    /// 最後の更新から `agent_timeout_ms` 以上たったセッションを捨てる。
    /// 捨てた分だけ「最後の操作」を `max(最後の操作, 更新 + timeout)` に進める。
    fn expire_sessions(&mut self, now_ms: u64) {
        let timeout = self.config.agent_timeout_ms;
        let mut last_op = self.last_op_ms;
        self.sessions.retain(|_, s| {
            let expires_at = s.updated_ms.saturating_add(timeout);
            if now_ms >= expires_at {
                last_op = last_op.max(expires_at);
                false
            } else {
                true
            }
        });
        self.last_op_ms = last_op;
    }

    /// 睡眠 → 散歩 → 待機（`held` が true のあいだは常に待機）。
    fn autonomous(&mut self, now_ms: u64) -> Output {
        if self.held {
            self.auto = None;
            return still(Pose::Idle);
        }
        let idle_ms = now_ms.saturating_sub(self.last_op_ms);
        if idle_ms >= self.config.sleep_after_ms {
            self.auto = None;
            return still(Pose::Sleep);
        }
        if idle_ms < self.config.walk_after_ms {
            self.auto = None;
            return still(Pose::Idle);
        }
        self.walk_or_rest(now_ms)
    }

    /// 散歩と休みを交互に進める。1 回の `tick` で切り替えは高々 1 段。
    fn walk_or_rest(&mut self, now_ms: u64) -> Output {
        let can_advance = self.advanced_at_ms != Some(now_ms);
        let next = match self.auto {
            None => Some(self.begin_walk(now_ms)),
            Some(Auto::Walk {
                started_ms, len_ms, ..
            }) if can_advance && now_ms.saturating_sub(started_ms) >= len_ms => {
                Some(self.begin_rest(now_ms))
            }
            Some(Auto::Rest {
                started_ms, len_ms, ..
            }) if can_advance && now_ms.saturating_sub(started_ms) >= len_ms => {
                Some(self.begin_walk(now_ms))
            }
            Some(_) => None,
        };
        if let Some(next) = next {
            self.auto = Some(next);
            self.advanced_at_ms = Some(now_ms);
        }
        match self.auto {
            Some(Auto::Walk { right: true, .. }) => Output {
                pose: Pose::RunRight,
                vx: self.config.walk_speed,
            },
            Some(Auto::Walk { right: false, .. }) => Output {
                pose: Pose::RunLeft,
                vx: -self.config.walk_speed,
            },
            _ => still(Pose::Idle),
        }
    }

    /// 乱数は長さ → 向きの順に引く。
    fn begin_walk(&mut self, now_ms: u64) -> Auto {
        let len_ms = self
            .rng
            .range(self.config.walk_min_ms, self.config.walk_max_ms);
        let right = self.rng.next_u64() & 1 == 0;
        Auto::Walk {
            started_ms: now_ms,
            len_ms,
            right,
        }
    }

    fn begin_rest(&mut self, now_ms: u64) -> Auto {
        let len_ms = self
            .rng
            .range(self.config.rest_min_ms, self.config.rest_max_ms);
        Auto::Rest {
            started_ms: now_ms,
            len_ms,
        }
    }
}

/// 止まっているポーズ（vx = 0）。
fn still(pose: Pose) -> Output {
    Output { pose, vx: 0.0 }
}
