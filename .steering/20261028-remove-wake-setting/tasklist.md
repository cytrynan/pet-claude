---
type: Steering
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-28T03:40:00+09:00 }
---

# 設定画面の「呼びかけで音声入力を始める」の欄を外す（T1）

## brief
- K 指示（2026-10-28）「設定の呼びかけで音声入力を始めるの欄の削除も行って」。ウェイクワードは保留中（K 2026-10-26）で、保存するだけの欄と「Phase 4 で使えます」の注記が残っていた。
- 変更: `ui/settings.html`（行と注記・JS の `wake`）／`state.rs`（`wake` を外す。旧い state.json の `wake` は読まない）／`main.rs` `settings_set`・`pet_window.rs` `set_settings`・`settings_get`（`wake` を外す）／`render-ui.js`・`clips-sweep.sh`・`pinch-drop.sh` の偽データ／README の予定。
- 残すもの: 「ウェイクワード」の表示行（ペットごとの呼びかけの言葉）。保留の再開に備える。

## タスク
- [x] 欄とコードから `wake` を外す (#120)
- [x] ゲート GATE OK・render-ui の設定画面（明暗）で欄が無いことを確認
- [x] gate-commit → PR（マージは K）

## 止まった点
- `pet_window.rs` の `settings_get` の既定値（JSON 直書き）に `"wake": false` が 1 つ残っていて、grep の 2 回目で見つけた。検索語は `wake` の単語境界でなく `"wake"` でも引くこと
