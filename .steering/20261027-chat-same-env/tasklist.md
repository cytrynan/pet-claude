---
type: Steering
description: チャットを Claude Code と同じ環境で起こす作業のタスクリスト。行末の番号は GitHub issue
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-28T01:30:00+09:00 }
---

# タスクリスト: 20261027-chat-same-env

tracking: #81

- [x] `claude -p` の alias（opus/fable/sonnet/haiku）と `--effort`・`--permission-mode`・`--setting-sources`・`--append-system-prompt-file` が通ることを確かめる (#82)
- [x] pet-core: `Model`・`Effort`・`PermissionMode`・`ChatOptions`・新しい `child_args`・`work_dir`（テスト 97→109 本） (#83)
- [x] pet-app: `state.json` に既定 4 項目、`ChatSetup` で送信、メニューの上書き、作業フォルダ、フォルダ選択ダイアログ (#84)
- [x] 設定画面にモデル・エフォート・モード・作業フォルダ (#85)
- [x] チャット欄の下の行に 2 つの切替メニュー（Claude アプリ風・明暗） (#86)
- [x] 検証: `cargo test`・chat-fault.py（18→23 項目）・render-ui.js（設定とチャット欄の新 UI）・ゲート (#87)
- [x] 実機: 同じ環境の `claude -p` で ①答え ②トークン数と秒数 ③承認のカード を数値で残す。15 秒超なら常駐案を設計だけ書く (#88)
- [x] docs（architecture §6.1・§7・DESIGN.md・log.md）と apple-design の改善点、PR (#89)

## 振り返り

- 計画どおり: pet-core の型と引数 → state と設定画面 → チャット欄のメニュー → 検証 → 実機 → docs の順で、1 セッション・サブエージェント 0 体
- 実測は実装の前に回し始めた（alias の確認 4 回・同じ環境の `-p` 2 回・印を書く hook 2 回・常駐ペットの入れ替え 1 回）。印を書く hook（使い捨てのプロジェクト設定）で「`--permission-prompts none` でも hook の allow が効く」を先に確かめたので、`host` への切替を作らずに済んだ
- 止まった点: ①`.island { inset: 12px }` を継いだメニューが窓の高さに伸び、`offsetHeight` を Rust に伝える設計が循環した（撮影で下に空きが出て気づき `top: auto` で解消） ②`<button>` の行が中身の幅に縮み、チェックと番号が行ごとにずれた（親を flex の列に） ③chat-fault.py は別セッションが同じ試験用ミューテックスの pet-app を起こしている間「既に起動しているので終了」で落ちる（単独で再実行して PASS）。`nokey` は K の資格情報マネージャーに鍵が保存されていると通らない（環境依存・main でも同じ） ④Bash のヒアドキュメントに `'` を含む長い Python を入れるとツール側で止まる → スクリプトは Write で書いて実行する
- 数値: tests 97→109・chat-fault 18→23（22 PASS＋環境依存 1）・render-ui の新しい場面 5（明暗で 10）・同じ環境の 1 送信 23.5 秒／66,772 トークン・カードまで 9.7 秒
- 残り（K の判断）: 子の常駐（brief の案）・ペット自身のチャットから来た Write/Edit の承認に「許可」を出す（brief「見つけた穴」）
