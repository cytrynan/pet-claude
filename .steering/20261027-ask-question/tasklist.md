---
type: Steering
description: 質問（AskUserQuestion）にペットで答える機能のタスクリスト。行末の番号は GitHub issue
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-27T02:30:00+09:00 }
---

# タスクリスト: 20261027-ask-question

tracking: #37

- [x] pet-core::ask をテスト先行で作る（質問の解析・検査・答えから hook の出力・1 問ずつの進み） (#38)
- [x] pet-hook: `--ask` の時だけ質問をアプリへ送り、答えを待って出力する（故障注入つき） (#39)
- [x] pet-app: 受け口に質問を入れ、吹き出しに質問のカード（選択・複数選択・拒否・ターミナルで・回答＝チャット欄） (#40)
- [x] 設定「質問にペットで答える」（既定オン・オフなら「聞きたいことがあるよ」と読み上げ） (#41)
- [x] `--install-hooks` に AskUserQuestion の同期 hook（timeout 300・`--ask`）を足す (#42)
- [x] 画面の確認（render-ui.js に質問のカード・明暗ではみ出し 0）と CLI の対話で A1〜A7（未確認②の自由文を含む） (#43)
- [x] docs（architecture §5・DESIGN.md・PRD・log.md）と apple-design の改善点 (#44)
- [ ] K の実機でデスクトップ版の確認（未確認①。手順は報告に） (#45)
