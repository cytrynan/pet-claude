---
type: Steering
description: Phase 1 の行動ロジック（pet-core::behavior）の要求。テスト作成係と実装係の唯一の原本
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-21T09:00:00+09:00 }
---

# 要求: ペットの行動ロジック（`pet-core::behavior`）

ペットがいま何のポーズを取り、どちらへ歩くかを決める純粋なロジック。時刻・利用者の操作・Claude Code の出来事を入力にとり、毎回「ポーズ」と「歩く速さ」を返す。OS にも Win32 にも触らない。乱数は種を渡して決定的にする。

## 識別子契約（この名前と型を変えない）

ファイル: `crates/pet-core/src/behavior.rs`（`lib.rs` に `pub mod behavior;` を足す）。既存の `pet_core::sprite::Pose` を使う。

**テストは `crates/pet-core/tests/behavior.rs`（統合テスト）に置き、`pet_core::behavior::*` と `pet_core::sprite::Pose` の公開 API だけを使う。`src/behavior.rs` の中にテストを書かない**（テスト作成係と実装係が同じファイルを書かないため）。

外部クレートを足さない（`Cargo.toml` を変えない）。

```rust
use crate::sprite::Pose;

/// 調整できる定数。`Default` は下の「既定値」の値。
#[derive(Debug, Clone, PartialEq)]
pub struct BehaviorConfig {
    pub walk_after_ms: u64,      // 無操作がこれだけ続いたら散歩を始めてよい
    pub sleep_after_ms: u64,     // 無操作がこれだけ続いたら眠る
    pub walk_min_ms: u64,        // 1 回の散歩の長さの下限
    pub walk_max_ms: u64,        // 同 上限
    pub rest_min_ms: u64,        // 散歩と散歩のあいだの休み（待機）の下限
    pub rest_max_ms: u64,        // 同 上限
    pub walk_speed: f32,         // 論理ピクセル毎秒（正の値）
    pub agent_timeout_ms: u64,   // Claude の作業状態をこれだけ更新が無ければ捨てる
}
impl Default for BehaviorConfig { /* 既定値 */ }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserEvent {
    Click,      // ペットがクリックされた（ドラッグではない）
    DragStart,  // つまみ上げた
    DragEnd,    // 離した
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    Prompt { session: String },     // 利用者がプロンプトを送った＝作業を始めた
    ToolStart { session: String },  // ツールの実行を始めた
    NeedsInput { session: String }, // 承認・回答など利用者の操作を待っている
    Done { session: String },       // 応答が終わった
    Error { session: String },      // 失敗した
}

/// `tick` の戻り値。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Output {
    pub pose: Pose,
    /// 横方向の速さ（論理ピクセル毎秒）。正は右、負は左、歩いていない時は 0.0。
    pub vx: f32,
}

pub struct Behavior { /* 非公開 */ }

impl Behavior {
    pub fn new(config: BehaviorConfig, seed: u64, now_ms: u64) -> Self;
    pub fn on_user(&mut self, event: UserEvent, now_ms: u64);
    pub fn on_agent(&mut self, event: AgentEvent, now_ms: u64);
    /// バー・メニューが開いている間 true。true の間は自律行動が待機になる（1 節の 4）。
    /// 値が変わったときだけ状態を更新する（同じ値を何度呼んでも何も起きない）。
    pub fn set_held(&mut self, held: bool, now_ms: u64);
    /// 歩いた結果、作業領域の端に当たって動けなかったときにアプリが呼ぶ。向きを反転する。
    pub fn on_blocked(&mut self, now_ms: u64);
    /// 毎フレーム呼ぶ。now_ms は単調に増える（同じ値で何度呼んでもよい）。
    pub fn tick(&mut self, now_ms: u64) -> Output;
}
```

## 既定値

| 項目 | 値 |
|---|---|
| walk_after_ms | 90_000 |
| sleep_after_ms | 300_000 |
| walk_min_ms / walk_max_ms | 3_000 / 8_000 |
| rest_min_ms / rest_max_ms | 10_000 / 30_000 |
| walk_speed | 40.0 |
| agent_timeout_ms | 120_000 |

## 振る舞い

### 1. 優先順位（上ほど強い。`tick` はいちばん上の当てはまるものを返す）

1. **つままれ**: `DragStart` から `DragEnd` までは `Pose::Grabbed`、vx=0
2. **一度だけの反応**（下の 2 種。再生中は他より優先。終わったら消える）
   - クリック: `Pose::Jumping`（喜ぶ）
   - 応答が終わった（`Done`）: `Pose::Jumping`
   - 失敗した（`Error`）: `Pose::Failed`
   - 反応の長さは `pose.native_clip()` の `cycle_ms × loops`（Jumping＝840×2＝1680ms、Failed＝1220×2＝2440ms）。`now_ms - 反応の開始 >= 長さ` になった `tick` からは反応を返さない
   - 反応中に新しい反応イベントが来たら、ポーズが同じ種類でも、新しい反応で置き換える（開始時刻も新しくする。2026-10-21 questions.md 1 の解釈 A を正とした）
   - 反応の開始時刻は、その `on_user`／`on_agent` に渡された `now_ms`。残り時間は壁時計で減り、1（つままれ）に隠れているあいだも消費される。`DragStart`／`DragEnd` は反応を消さない
3. **Claude の状態**（有効なセッションがあるとき。vx=0）
   - いずれかのセッションが「操作待ち」→ `Pose::Waiting`（こちらを見て手を振る）
   - そうでなく、いずれかが「作業中」→ `Pose::Running`
4. **自律行動**（上のどれも当てはまらないとき）: 睡眠 → 散歩 → 待機（下の 3〜6 節）。**`held` が true のあいだ、4 は常に待機（`Pose::Idle`・vx=0）**。1〜3 は `held` の影響を受けない

### 2. Claude のセッション状態

- セッションは `session` 文字列で区別する。セッションごとに状態（作業中／操作待ち）と最後に更新した時刻を持つ
- `Prompt` と `ToolStart` → そのセッションを「作業中」にする
- `NeedsInput` → 「操作待ち」にする
- `Done` → そのセッションの状態を消し、「応答が終わった」反応を始める
- `Error` → そのセッションの状態を消し、「失敗した」反応を始める
- 最後の更新から `agent_timeout_ms` 以上たったセッションは無かったものとして扱う（`tick` の冒頭で捨ててから優先順位を評価する。ちょうど等しいときも捨てる）
- 知らないセッションの `Done`／`Error` も反応は起こす

### 3. 無操作の時計

- 「最後の操作」の時刻を持つ。`new` の `now_ms` で初期化する。`held` とつままれ状態は `new` の時点で false
- 次のどれでも「最後の操作」を now_ms に更新する: `on_user` のすべて、`on_agent` のすべて、`set_held`（値が変わったとき。true でも false でも）
- **無操作の経過 ms** は、有効なセッションがある・`held` が true・つままれ中のいずれかなら 0、そうでなければ `now_ms − 最後の操作`
- `held` が false に変わったとき、`DragEnd` のときは、その `now_ms` を「最後の操作」にする（上の更新規則のとおり）
- セッションがタイムアウトで消えたときは、「最後の操作」を `max(最後の操作, そのセッションの最後の更新 + agent_timeout_ms)` にする（`tick` で消した時点で行う）
- `on_blocked` は「最後の操作」を更新しない

### 4. 睡眠

- 無操作が `sleep_after_ms` 以上続いたら `Pose::Sleep`、vx=0（散歩中でも眠る）
- 何かの操作（上の 3 節の更新）で起きる

### 5. 散歩

- 無操作が `walk_after_ms` 以上 `sleep_after_ms` 未満のあいだ、「散歩」と「休み」を交互に繰り返す
- 散歩に入れるようになった最初の `tick` で、まず 1 回散歩を始める。散歩の長さは `walk_min_ms..=walk_max_ms` から、休みの長さは `rest_min_ms..=rest_max_ms` から乱数で選ぶ
- **散歩・休みの開始時刻は、その状態に入る条件を最初に満たした `tick` の `now_ms`（遡らない）**。散歩は `now_ms − 開始 >= 散歩の長さ` になった `tick` から休みに、休みは `now_ms − 開始 >= 休みの長さ` になった `tick` から散歩に切り替わる。1 回の `tick` で切り替えは高々 1 段。同じ `now_ms` で `tick` を繰り返しても 2 回目以降は状態を進めず、乱数も引かない（間に `on_user`／`on_agent`／`set_held`／`on_blocked` が入れば、次の `tick` は同じ `now_ms` でも新しい状態で評価する）
- **乱数**: 外部クレートを足さず自前（SplitMix64 など）で実装し、`seed` だけに依存させる。テストは seed と向き・長さの対応を仮定しない: 長さは `walk_min_ms == walk_max_ms`・`rest_min_ms == rest_max_ms` にして固定し、向きは `RunRight`（vx=+walk_speed）と `RunLeft`（vx=-walk_speed）のどちらでもよい書き方にする（pose と vx の符号が整合していることを検査）
- 散歩の向きは乱数で右か左。散歩中は右なら `Pose::RunRight`・vx=+walk_speed、左なら `Pose::RunLeft`・vx=-walk_speed
- 休みのあいだは `Pose::Idle`・vx=0
- `on_blocked` が呼ばれたら、散歩中なら向きを反転する（散歩の残り時間はそのまま）。散歩中でなければ何もしない
- 操作で無操作の時計が戻ったら、散歩も休みも打ち切る（次に散歩に入れるようになったら、また散歩から始める）
- 同じ seed・同じ入力列（`tick` の `now_ms` 列を含む）なら、同じ出力列になる（決定的）。テストは同じ seed の `Behavior` を 2 つ回して `Output` 列の一致で検査する

### 6. 待機

- 上のどれでもないときは `Pose::Idle`、vx=0

## 範囲外

- 窓の移動・クランプ・DPI（アプリ側の仕事）
- 承認の許可/拒否（Phase 2）。`AgentEvent` に承認の ID は持たせない
- アニメーションのコマ送り（`sprite::Playback` の仕事）
- 不正な設定（`walk_min_ms > walk_max_ms` など）の扱いは未定義（テストしない）
- `DragStart` が続けて来た・`DragStart` 無しの `DragEnd` は、つままれ状態を壊さない範囲で無視してよい（テストしない）

## 受け入れ

- `cargo test -p pet-core` が通る
- `cargo clippy --workspace --all-targets -- -D warnings` が通る
- 上の各節に、少なくとも 1 本ずつ対応するテストがある
