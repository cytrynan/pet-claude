---
type: Steering
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-21T18:45:00+09:00 }
---

# 設計（動画クリップの再生）

## 1. `pet-core::clips`（OS に触らない純粋なロジック）の公開 API

テストはこの API に対して書く。実装はこの形に合わせる。

```rust
use crate::sprite::Pose;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Facing { Left, Right, Front }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Still { Idle, Walk, Sleep, Dragged, Attention, Working }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VState { pub still: Still, pub facing: Facing }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipKind { Loop, Transition, Once }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipDef {
    pub name: String,
    pub kind: ClipKind,
    pub from: VState,
    pub to: VState,
    pub frames: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipSet { pub fps: u32, pub clips: Vec<ClipDef> }

impl ClipSet {
    /// clips.json を読む。fps が 0、frames が 0、名前の重複、ループ・単発で from≠to、
    /// つなぎで from＝to、Front 以外の向きの Dragged・Attention、は Err。
    pub fn from_json(text: &str) -> Result<ClipSet, String>;
}

/// いま描くもの。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Show {
    /// `clips[index]` のコマ `frame` を描く。`mirror` なら左右反転して描く
    Clip { index: usize, frame: u32, mirror: bool },
    /// クリップを使わずシートの行で描く（Failed、またはつなぎ無しで来たループの無い姿勢）
    Sheet(Pose),
}

pub struct Player { /* 非公開 */ }

impl Player {
    /// 最初は、`pose` の目標の姿勢につなぎ無しで置く（向きは Left）。
    pub fn new(set: ClipSet, pose: Pose, now_ms: u64) -> Player;
    /// 行動が求めるポーズを伝える（毎 tick 呼んでよい。同じポーズなら何もしない）。
    pub fn set_pose(&mut self, pose: Pose, now_ms: u64);
    /// いま描くものを返す。時刻は単調に増える前提。
    pub fn tick(&mut self, now_ms: u64) -> Show;
    /// 窓を横に動かしてよいか（Walk のループを流している間だけ true）。
    pub fn may_move(&self) -> bool;
    pub fn clips(&self) -> &ClipSet;
}
```

`Pose` に `Petted` を足す（`sprite.rs`）。シートでは `Jumping` の行で表示する。

### 時刻の決め方（テストが値を決められるように）
- ループの最中に `set_pose` でポーズが変わったら、次のクリップはその `now_ms` から始まる。
- つなぎ・単発が終わったら、次のクリップは**終わった時刻**（開始 + `frames × 1000 / fps`、ms は切り捨て）から始まる。`tick` を呼んだ時刻ではない。
- 1 回の `tick` で複数のクリップの終わりを越えたら、越えた分だけ順に進める。
- 単発は、そのポーズに入るたびに 1 回だけ流す。流し終えてもまだ同じ単発のポーズなら、Idle のループを流す。
- つなぎ無しで「どちらでもよい」向きの目標へ切り替えるときは Left を選ぶ。
- Failed はシートで表示し、見た目の状態（VState）は変えない。

### 例（テストの雛形として使ってよい）
13 本の割り当ては requirements.md の表と同じ。fps は 8。コマ数の例: idle 37・walk 37・working 37・grabbed 22・stop 14・walk_start 14・turn 14・sleep 22・wake 22・put_down 14・call 22・happy 22・petted 22。テストでは `ClipSet` を直接組み立ててよい（コマ数は自由に決めてよい）。

## 2. 素材の作り方（`scripts/build_clips.py`）
1. ffmpeg で 16fps の元動画を RGB のコマにする。最後の 3 コマを捨て、1 コマおきに取って 8fps にする。
2. 白背景を抜く: 端から、どの色も 236 以上の画素を塗りつぶして背景とする。背景に接する縁の画素は、白との混ざり具合からアルファと色を戻す。
3. 位置と大きさ: クリップの 1 コマ目の絵の範囲（高さ・下端・横の中心）を、始点の姿勢画像（`poses/verification.json`）に合わせる変換を求め、全コマに同じ変換をかける。次に全クリップ共通の倍率で 384×416 のコマに縮め、床を y=404、中心を x=192 に置く。共通の倍率は「待機の高さをシートの待機と同じ 354px にする倍率」と「全コマが 384×416 に 98% で収まる倍率」の小さい方。
4. `clips/<name>/000.webp`（非可逆 quality 92・アルファは可逆）と `clips.json` を書く。

## 3. アプリ（`pet-app`）
- 読み込み: `clips.json` を読み、`ClipSet` と各コマのパスだけを持つ。コマは、流すときに別スレッドでクリップ単位で読み、表示の解像度に縮めて不透明な範囲で切り抜いて持つ（`CellStore` と同じ方式）。持つクリップは、使った順で、合計が上限（12MB）を超えたら古いものから捨てる。いま流しているクリップは捨てない。
- まだ読めていないクリップを流す番になったら、読み終わるまで直前のコマを出したままにする（`Player` は時刻で進むので、読み終わった時点のコマから出る）。
- シートの `CellStore` は、クリップがあるペットでは Failed の行だけ持つ（ほかの行は使わない）。
- 窓の移動: `behavior` の vx は `Player::may_move()` が true の時だけ使う。
- 呼吸: シートで描くとき（Sleep・Attention で止めたコマを含む）だけ付ける。ループには付けない（動画に動きがあるため）。

## 4. テスト作成係が挙げた曖昧な点の決定（2026-10-21 司令塔）
1. 終わった時刻の ms 切り捨てと規則 9 の「経過 ≥ frames×1000/fps」は、どちらも整数の `frames * 1000 / fps`（切り捨て）で判定する。fps が 1000 を割り切らなくても同じ値を使う。
2. `may_move()` は直前の `tick` の結果に対する値。`set_pose` だけでは変わらない。
3. `Player::new` に向きの決まるポーズ（RunRight・RunLeft）を渡したら、その向きで置く。向きが「どちらでもよい」ポーズは Left。
4. Failed は規則 7 の「それ以外」に含める（つなぎ・単発の最中なら最後まで待ってから表示）。
5. Failed の最中の `may_move` は false。
6. Failed・単発・つなぎの後に同じ姿勢のループへ戻るときは、ループを 0 コマ目から（戻った時刻を始点に）流し直す。
7. `clips.json` は `from` と `to` の両方を必須にする。
