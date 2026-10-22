---
type: Steering
description: Phase 3 後半（読み上げ・音声入力）のタスクリスト。行末の番号は GitHub issue
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-22T09:45:00+09:00 }
---

# タスクリスト: 20261022-phase3-voice

- [ ] K に計画を見てもらう（Q1 文字起こしの道具・Q2 エンジンの起動・Q3 文字起こしの後）
- [ ] `pet-core::voice` をテスト先行で作る（話す文・話す順・録音の区切り・文字起こしの整形・WAV）
- [ ] 読み上げ: `pet-app::tts`（エンジンとの往復・鳴らす・止める・エンジンの起動と停止）と、答え・固定文言・承認から呼ぶ
- [ ] 子プロセスの起こし方を `child.rs` に移す（チャットの振る舞いは変えない）
- [ ] 音声入力: 録音（waveIn）・文字起こしの子・チャットへ送る・バーの見た目
- [ ] 準備のスクリプト `scripts/setup-stt.ps1` と `scripts/stt/transcribe.py`
- [ ] 設定画面に読み上げと音声入力の状態
- [ ] 故障注入（V2・V8・V9・V10）と、チャットの故障注入 18 項目の再走
- [ ] 実機の確認（S11 を K の声で・声のオフで止まる・N1）
- [ ] docs: PRD F16・F17、architecture §6、roadmap、log.md
- [ ] 検証係（Codex）の確認
