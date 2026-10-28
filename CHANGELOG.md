# Changelog

このプロジェクトの全ての注目すべき変更はこのファイルに記載されます。

書式は [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) に基づき、
このプロジェクトは [Semantic Versioning](https://semver.org/lang/ja/) に従います。

## [Unreleased]

### Added
-

### Changed
-

### Deprecated
-

### Removed
-

### Fixed
-

### Security
-

## [1.0.0] - 2026-10-28

最初のリリース。配布は zip（`scripts/release.ps1` が作る `pet-claude-1.0.0-windows-x64.zip`）。

### Added
- ペット本体（Win32 の layered window）とトレイ常駐、ペット 6 体（まめドラゴン・りんりん・とらちゃん・しばちゃん・こむぎちゃん・ごまちゃん）と姿勢の間をつなぐ動画クリップ
- Claude Code hooks との連携（作業中・完了・失敗・入力待ちのポーズと吹き出し、承認と質問のカード）。登録は `pet-app.exe --install-hooks`
- チャット（Claude Code と同じ環境の `claude -p`。モデル・エフォート・権限モードをチャット欄で切替）、音声入力（faster-whisper）、読み上げ（AivisSpeech）
- 設定画面、サインイン時の自動起動（Windows タスク）、PDF マニュアルと `docs/usage.md`
- 配布 zip の初回セットアップ手順 `SETUP.md`（Claude Code が読んで実行する形。hooks 登録・デスクトップのショートカット・起動確認）と `scripts/release.ps1`（zip を作って展開し直し、同梱物と exe と hooks の登録を自己検査）

### Changed
- アプリのアイコン（exe・トレイ・ショートカット）を仮キャラの絵からまめドラゴンの待機の 1 コマ目に変更（`scripts/make-icon.py`）
