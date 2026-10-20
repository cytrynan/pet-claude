# pet-claude

Claude Code の作業状態に合わせて動く、Windows 用のデスクトップペット。ChatGPT デスクトップのペット機能を Claude Code 向けに作り直したもの。声は AivisSpeech。

## できること（予定）

- 他のウィンドウの上に浮かび、フォーカスを奪わない。キャラクターの不透明部分だけがクリックに反応する
- Claude Code の状態（作業中・承認待ち・完了・失敗）に合わせて動く
- 承認待ちはペットの吹き出しから許可・拒否を返せる
- 話しかけると声で返す（AivisSpeech・天深シノ）
- 絵は Codex ペット互換のスプライトシート。差し替えても振る舞いは変わらない

## 使い方

Phase 0 の実装中。手順は実装後にここへ書く。

## 技術の詳細

- Python 3.12 + PySide6（Qt 6）。単一プロセスでトレイ常駐
- Claude Code hooks → loopback HTTP → アプリ。hook は常に exit 0 で、アプリが居なくても Claude Code を止めない
- 計画: [docs/roadmap.md](docs/roadmap.md)。要件: [docs/product-requirements.md](docs/product-requirements.md)。設計: [docs/architecture.md](docs/architecture.md)。参考にした OpenPets の解析: [docs/openpets-analysis.md](docs/openpets-analysis.md)

## ライセンス

MIT
