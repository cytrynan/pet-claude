---
type: Steering
description: Phase 4 のタスクリスト。行末の番号は GitHub issue
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-23T02:50:00+09:00 }
---

# タスクリスト: 20261023-phase4

tracking: #26

- [ ] K に Q1（ダウンロード）・Q2（本番素材）を聞く。答えを待つ間は聞かなくてよい所を進める
- [x] 自動起動: 設定のスイッチで Windows タスクを登録・解除（P1〜P4） (#27)
- [ ] 自動起動: K の実機でサインインしてペットが出る（P5） (#28)
- [ ] ウェイクワード: AivisSpeech で学習データを合成する道具 (#29)
- [ ] ウェイクワード: K の録音スクリプト（record-wake.ps1） (#30)
- [ ] ウェイクワード: 特徴抽出・学習・判定（oww.py / train.py / evaluate.py） (#31)
- [ ] ウェイクワード: マイクの音を検出の子につなぐ（PR #25 のマージ後） (#32)
- [ ] 本番素材: U3 を決める（Q2） (#33)
- [ ] docs: PRD・architecture・roadmap・log を実装どおりに (#34)

## 実績

| 受け入れ基準 | 判定 | 証拠 |
|---|---|---|
| P1 登録・置き換え・解除で 1 つだけ | PASS | 2026-10-23 02:38 無効の試験タスク `pet-claude-test-autostart` で Off→登録→On→もう一度登録→解除→Off→もう一度解除（何もしない）。最後に `schtasks /Query` で残っていない。本物の `pet-claude` は作っていない |
| P2 タスクの中身 | PASS | 単体 12 本（`crates/pet-core/tests/autostart.rs`）＋登録したタスクを `Get-ScheduledTask` で読む: Enabled False（試験）・ExecutionTimeLimit PT0S・Priority 4・IgnoreNew・電池で止めない・`%SystemRoot%\System32\wscript.exe //B //Nologo "…\run-hidden.vbs" pet-claude "…\pet-app.exe"`・UserId PC-CY\meer0・Delay PT10S・Interactive・Limited |
| P3 画面は実物に合わせる | PASS（見た目）／未測（実機の WebView2） | `render-ui.js` の settings-autostart-on・other・error を明暗 2 通り。設定画面は元から API キーの時 633px（窓は 600px）でスクロールしていた。起動の節で 743px |
| P4 窓が出ずにペットが出る | PASS | 試験の置き場所（`PET_CLAUDE_DATA_DIR`）で `wscript //B //Nologo run-hidden.vbs pet-claude-test <exe>` → 9 秒後に pet-app が動き、見える窓は `pet-claude`（ペット）だけ。試験のログに起動の行。止めた後に子は残っていない |
| P5 サインインで出る | 未測 | K の実機 |
