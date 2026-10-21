---
type: Steering
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-21T18:40:00+09:00 }
---

# 動画クリップの再生（まめドラゴンの 13 本を組み込む・T2）

## brief
- K 指示（2026-10-21）: Comfy Cloud で作ったまめドラゴンの動画 13 本（ループ 5・つなぎ/単発 8）を pet-claude に組み込む。
- 動画の在処: `../20261021-wan-transitions/`（run1・run2・run4）と `../20261021-wan-walk-trial/`。なでられは `run4/mame_petted_d.mp4`（K 採用）。
- Codex 互換のシート（1 行 8 コマ）はそのまま残す。動画はペットのフォルダに**追加の素材**（クリップ）として置き、`pet.json` に `clips` が無いペットは今までどおり動く。
- 姿勢の間を動画でつなぐ。今いる「姿勢」から、行動が求めるポーズの姿勢まで、つなぎの動画を順に流す。

## 用語
- **姿勢（Still）**: Idle・Walk・Sleep・Dragged・Attention・Working の 6 つ。動画の始点と終点はどれかの姿勢。
- **向き（Facing）**: Left・Right・Front。Dragged と Attention は Front。
- **見た目の状態（VState）**: (姿勢, 向き)。
- **クリップ**: 1 本の動画をコマにしたもの。`from` と `to` の VState を持つ。種類は 3 つ。
  - ループ: from＝to。ポーズの間ずっと繰り返す。
  - つなぎ: from≠to。1 回流して to の姿勢になる。
  - 単発: from＝to だが 1 回だけ流す（喜ぶ・なでられ）。
- **反転**: クリップを左右反転して流すと、向きの Left と Right が入れ替わる（Front は Front のまま）。

## 素材の形式（ペットのフォルダ）
- `pet.json` に `"clips": "clips.json"` を足す（任意の項目）。
- `clips.json`: `{"fps": 8, "clips": [{"name", "kind": "loop"|"transition"|"once", "from": ["idle","left"], "to": [...], "frames": N}]}`
- コマは `clips/<name>/000.webp` から連番。1 コマの大きさはシートのコマと同じ（`cellScale` 倍の 192×208）。透過 WebP。

## 13 本の割り当て
| クリップ | 種類 | from → to |
|---|---|---|
| idle | ループ | (Idle, Left) |
| walk | ループ | (Walk, Right) |
| working | ループ | (Working, Left) |
| grabbed | ループ | (Dragged, Front) |
| stop | つなぎ | (Walk, Right) → (Idle, Right) |
| walk_start | つなぎ | (Idle, Right) → (Walk, Right) |
| turn | つなぎ | (Idle, Right) → (Idle, Left) |
| sleep | つなぎ | (Idle, Left) → (Sleep, Left) |
| wake | つなぎ | (Sleep, Left) → (Idle, Left) |
| put_down | つなぎ | (Dragged, Front) → (Idle, Left) |
| call | つなぎ | (Idle, Left) → (Attention, Front) |
| happy | 単発 | (Idle, Left) |
| petted | 単発 | (Idle, Left) |

## 計画の規則（`pet-core::clips`。テスト先行の対象）
ポーズ → 目標:
- Idle → (Idle, 今の向き。今が Front なら「どちらでもよい」)
- RunRight → (Walk, Right)／RunLeft → (Walk, Left)
- Sleep → (Sleep, 今の向き。Front なら「どちらでもよい」)
- Grabbed → (Dragged, Front)
- Waiting・Waving → (Attention, Front)
- Running・Review → (Working, 今の向き。Front なら「どちらでもよい」)
- Jumping → 単発 happy／Petted → 単発 petted（どちらも Idle の姿勢で流し、終わったら次のポーズへ）
- Failed → クリップを使わずシートの行で表示する

経路:
1. 今の VState から目標の VState まで、つなぎクリップ（反転も含む）をつないだ最短の列を選ぶ（クリップの本数が最少。同数なら反転の少ない方、それも同じなら clips.json の並びが先の方）。
2. 経路が無いときは、目標の姿勢へ切り替える（つなぎ無し）。
3. 「どちらでもよい」向きの目標は、向きを問わずに最短の列を選ぶ。
4. 目標の姿勢にループがあれば、着いたらループを流す（向きが違えば反転して）。ループが無い姿勢（Sleep・Attention）は、そこへ来たつなぎの最後のコマを止めて表示する。つなぎ無しで来たならシートの行で表示する。
5. 単発は、まず Idle（今の向き）まで行き、単発を流す（向きが Right なら反転）。

割り込み:
6. Grabbed は、つなぎ・単発の途中でも**すぐに**切り替える（向きは Front）。
7. それ以外のポーズの変化は、流しているつなぎ・単発を最後まで流してから、その終点から計画し直す。ループの最中なら、その場で計画し直す。
8. つなぎ・単発の最中に何度ポーズが変わっても、計画し直すときは**最後の**ポーズを使う。

時間:
9. コマ番号: ループは `(経過ms × fps / 1000) mod コマ数`。つなぎ・単発は `min(経過ms × fps / 1000, コマ数 − 1)`、経過ms が `コマ数 × 1000 / fps` 以上で終わり。
10. 窓を横に動かしてよいのは Walk のループを流している間だけ（つなぎの最中は止まる）。

## 行動の変更（`pet-core::behavior`）
- クリック → 新しいポーズ `Petted`（なでられ）。完了（Done）は今までどおり `Jumping`（喜ぶ）。`Petted` はシートでは `Jumping` の行で表示する。反応の長さは `Jumping` と同じ。

## 受入基準
- R1 13 本がまめドラゴンに入り、`clips` を持たないペット（仮キャラ）は今までどおり動く。
- R2 計画の規則 1〜10 をテストで確かめる（テスト先行・テスト作成係は実装を見ない）。
- R3 背景が透過になっている（白が残らない）。動画の最後のかすんだ 3 コマ（16fps の元で）を使わない。
  - 例外（2026-10-21 実装時に決定）: walk_start は末尾を捨てない。捨てると終点の姿勢まで届かず、次の walk ループとの継ぎ目で頭が 24px 跳ねた。walk_start の最後のコマは彩度が下がっていない（最初 20.0・最後 21.6。ほかの動画は 2〜3 下がる）ので、かすみの心配も無い。
- R4 姿勢の切り替えで大きさと足元がずれない（つなぎの始点・終点と、ループの 1 コマ目の足元の差が 4px 以内・コマの大きさで）。
- R5 PRD N1 を守る: 普段の専有 50MB 以下（K の大きさ 105%・200% の画面）、CPU 1 コアの 2% 以下（待機中の平均）。
- R6 品質ゲート（fmt・clippy -D warnings・test）が通る。実機で待機・散歩・つまむ・置く・クリック・眠る・起きるを撮影して確かめる。

## 非スコープ
- 困る（Failed）の動画は作らない（シートの行のまま）。
- 作業中に入る・出るつなぎ、呼びかけから戻るつなぎ（今回の 13 本に無い。切り替えで表示する）。
- Codex のペット素材の形式の変更。
