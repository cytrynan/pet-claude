---
type: Steering
description: Phase 3 前半（チャット）のタスクリスト。行末の番号は GitHub issue
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-21T21:55:00+09:00 }
---

# タスクリスト: 20261021-phase3-chat

tracking: #6

- [x] K に計画を見てもらう（Q1・Q2 とも推奨で決定。K 2026-10-21）
- [ ] 実機で引数を確かめる: `--tools ""`・`disableAllHooks`・`--permission-prompts none` が手元の CLI で効くか、サブスクでの初回の秒数 (#7)
- [ ] `pet-core::chat` をテスト先行で作る（引数・子の環境・出力 JSON の解析・吹き出しの切り詰め） (#8)
- [ ] `state.json` に `chat_auth`、資格情報マネージャーへの鍵の保存・削除 (#9)
- [ ] 設定画面: 接続の選択・鍵の入力・「いまの接続を確かめる」 (#10)
- [ ] `chat_submit` を子プロセスにつなぐ（別スレッド・1 本だけ・120 秒・Job Object・考え中の動き） (#11)
- [ ] 吹き出しとチャット欄に答えを出す・「新しいチャット」 (#12)
- [ ] 故障注入（A6・A7）と実機の確認（A1〜A5・A9） (#13)
- [ ] docs: PRD F18・F19、architecture §6 と §7 を実装どおりに、log.md (#14)
- [ ] 検証係（Codex）の確認 (#15)
