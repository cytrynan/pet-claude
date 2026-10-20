---
type: Roadmap
description: pet-claude の段階計画。Phase 0（窓の実測）→ 1（行動）→ 2（Claude Code 連携・承認）→ 3（声・チャット）→ 4（ウェイクワード・本番素材）
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-21T08:45:00+09:00 }
stale_after: 2027-01-21
---

# ロードマップ

各 Phase は `.steering/<日付>-<名前>/` を 1 つ作り、受け入れ条件（PRD §7）を満たしたら次へ進む。

## Phase 0: 窓の土台を 2 画面×倍率違いで実測する（2026-10-21 実施・12 項目合格）

- Rust ワークスペース（`pet-core`・`pet-app`）、仮画像（手続き生成の 8 列×11 行）
- 透過・最前面・フォーカス非取得・不透明部分だけのクリック・ドラッグ・クランプ・ディスプレイ変化
- チェックリスト `docs/checklists/phase0-window.md`: W1〜W12 すべて合格。W10 は K の了承のうえ `DisplaySwitch` でノートだけ→拡張に戻して実測（記録 `.steering/20261021-phase0-window/`）
- 全画面アプリ: 前面の間は譲り、閉じたらペットが見える。別の最前面窓に覆われても 1.6 秒以内に上へ戻った（取り直しの周期は 1 秒）
- PRD §7 の「スクショ付き」は W1・W9・W10 のみ。残り（W2・W3・W4・W11）の扱いは K と決める

## Phase 1: 行動と素材規約

- `behavior/` の状態機械（待機・散歩・睡眠・つままれ＋Agent 状態の優先順位）を `cargo test` 先行で
- `assets/pets/<name>/pet.json` の規約と読み込み、行の代替
- 右クリックメニュー（設定／バーを非表示／終了）、トレイ、設定の保存
- クリック反応（喜ぶ・振り向く）

## Phase 2: Claude Code 連携と承認

- `crates/pet-hook`（Rust の小さな exe）、`settings.json` への管理マーカー付き登録・解除コマンド
- IPC（loopback HTTP＋token＋発見ファイル）と間引き
- 吹き出し（検証済み短文）と、`PermissionRequest` の許可・拒否の往復
- 故障注入（アプリ停止／token 不一致／応答遅延）

## Phase 3: 声とチャット

- AivisSpeech TTS（固定文言＋答え）、設定画面（大きさ・音声出力・音声トリガー）
- バー（チャット／音声入力）、ローカルで文字起こし（whisper-rs かサイドカーの faster-whisper。architecture §6）
- チャットの送り先（PRD U1）を K と決めてから Agent SDK 連携

## Phase 4: ウェイクワードと本番素材

- openWakeWord の独自モデル（K の録音）。誤反応率を実測
- Windows タスクで自動起動（`run-hidden.vbs`）
- 本番素材（Codex 互換のスプライトシート）に差し替え。絵は Codex に依頼（CLAUDE.md §3-A）
