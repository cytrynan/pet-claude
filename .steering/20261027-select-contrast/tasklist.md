---
type: Steering
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-27T17:55:00+09:00 }
---

# 設定画面のペット選択の一覧が読めない（T1）

## brief
- K 報告（2026-10-27・スクリーンショット）: 設定の「ペット」の選択肢一覧が白地に白〜薄灰色の文字で、選んでいる 1 行しか読めない。
- 原因: `ui/settings.html` の `.popup select` は文字色 `var(--label)` だけを指定し、`option` の地色は WebView2 が別窓で描く一覧まかせ。一覧の明暗が本文（暗い配色）と食い違うと白地に白文字になる。
- 直し: `.popup option { color: var(--label); background: var(--cell); }` を足し、文字色と地色を組で明示する（明暗どちらの一覧でも本文と同じ組になる）。
- 対象外: 一覧の明暗が食い違う根本原因（WebView2 側）。カスタムのドロップダウンへの置き換え。

## タスク
- [x] `.popup option` に文字色と地色を明示する (#77)
- [x] 実機で暗い配色の一覧 4 行が読めることをスクリーンショットで確認（`settings-open.png`。仮キャラ・まめドラゴン・りんりん・とらちゃん が暗い地に白文字。明るい配色は `--cell: #fff`＋`--label: #1d1d1f` の組で実機未確認）
- [ ] ゲート → gate-commit → PR（マージは K）

## 止まった点
- 自前の exe（pet-app）は computer-use の許可対象に名前で引っかからない（Start メニューに無い）。撮影は `winctl focus/click/shot` で代替した。pwsh（DPI 非対応）の GetWindowRect は論理座標＝物理の 1/2
