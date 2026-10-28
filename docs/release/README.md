---
type: Guide
description: 配布 zip の同梱 README。何が入っているかと、Claude Code に SETUP.md を渡す 1 行だけ
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-28T14:30:00+09:00 }
---

# pet-claude

Claude Code の作業に合わせて動く、Windows 11 用のデスクトップペット。承認や質問を吹き出しで受け、チャットと声でも話せる。

## はじめかた

1. この zip を、あとで動かさない場所に展開する（推奨 `%LOCALAPPDATA%\Programs\pet-claude`）
2. 展開したフォルダで Claude Code を開き、こう頼む

```
SETUP.md を読んで、書いてあるとおりに初回セットアップをして
```

Claude Code が hooks の登録・デスクトップのショートカット（まめドラゴンのアイコン）・起動確認まで進める。利用者が答えるのは「ペットが画面に出たか」の 1 点だけ。

## 入っているもの

| ファイル | 役割 |
|---|---|
| `pet-app.exe` | ペット本体（トレイに常駐） |
| `pet-hook.exe` | Claude Code の hooks から呼ばれる小さな exe。`pet-app.exe` と同じフォルダに置く |
| `pet-claude.ico` | ショートカット用のアイコン（まめドラゴン） |
| `pets\` | ペット 6 体（まめドラゴン・りんりん・とらちゃん・しばちゃん・こむぎちゃん・ごまちゃん） |
| `SETUP.md` | Claude Code 用の初回セットアップ手順 |
| `docs\usage.md` | 使い方（起動・操作・チャット・声・設定・困ったとき） |
| `docs\pet-claude-manual.pdf` | 画像つきのマニュアル |
| `scripts\run-hidden.vbs` | 自動起動で窓を出さずに起こす補助 |
| `scripts\setup-stt.ps1` | 音声入力の準備（任意） |

## 必要なもの

- Windows 11（WebView2 ランタイム同梱）
- Claude Code（ログイン済み）。チャットは `claude` コマンドを使う
- 任意: 音声入力に Python 3.10、読み上げに AivisSpeech（zip には入れていない）

## ライセンス

MIT（`LICENSE`）。ペットの絵は pet-claude の同梱素材で、このアプリで使う目的に限る。
