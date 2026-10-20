# pet-claude

Claude Code の作業状態に合わせて動く、Windows 用のデスクトップペット。ChatGPT デスクトップのペット機能を Claude Code 向けに作り直したもの。声は AivisSpeech。

## できること（予定）

- 他のウィンドウの上に浮かび、フォーカスを奪わない。キャラクターの不透明部分だけがクリックに反応する
- Claude Code の状態（作業中・承認待ち・完了・失敗）に合わせて動く
- 承認待ちはペットの吹き出しから許可・拒否を返せる
- 話しかけると声で返す（AivisSpeech・天深シノ）
- 絵は Codex ペット互換のスプライトシート。差し替えても振る舞いは変わらない

## 使い方

Phase 0（ペットの窓）まで。Rust（rustup）と VS 2022 の C++ ツールが要る。

```bash
cargo build -p pet-app --all-targets
./target/debug/pet-app.exe
```

トレイのアイコンから「位置をリセット」「終了」。

## 技術の詳細

- Rust。ペット本体は Win32 の layered window（透明な画素のクリックは OS が背後へ通す）、トレイと今後の UI は Tauri 2。1 プロセスでトレイ常駐
- Claude Code hooks → loopback HTTP → アプリ（Phase 2）。hook は常に exit 0 で、アプリが居なくても Claude Code を止めない
- 計画: [docs/roadmap.md](docs/roadmap.md)。要件: [docs/product-requirements.md](docs/product-requirements.md)。設計: [docs/architecture.md](docs/architecture.md)。参考にした OpenPets の解析: [docs/openpets-analysis.md](docs/openpets-analysis.md)

## ライセンス

MIT
