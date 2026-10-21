# pet-claude

Claude Code の作業状態に合わせて動く、Windows 用のデスクトップペット。ChatGPT デスクトップのペット機能を Claude Code 向けに作り直したもの。声は AivisSpeech で付ける予定（未実装）。

## できること

- 他のウィンドウの上に浮かび、フォーカスを奪わない。キャラクターの不透明部分だけがクリックに反応する。ドラッグで動かせて、画面の外へははみ出さない
- 待機・散歩・睡眠・つままれのポーズで動く。放置したときの散歩は設定でオン・オフでき、既定はオフ
- Claude Code の状態（作業中・入力待ち・完了・失敗）に合わせてポーズを変え、完了・入力待ち・失敗を吹き出しの短い文で知らせる
- 承認待ち（`PermissionRequest`）を吹き出しのカードに出し、「許可」「拒否」「ターミナルで」から答えを返せる。要約が見た目と食い違うおそれのある要求（見えない書式文字を含む、長すぎるなど）は、許可ボタンを出さずにターミナルで答えてもらう
- クリックでバー、右クリックかカーソルを乗せて 200ms でメニュー（設定・バーを非表示・位置をリセット・終了）を出す。設定では、ペットの種類と大きさ、散歩のオン・オフを変えられる
- ペットは設定画面で切り替えられる。同梱はまめドラゴンと、動作確認用の仮キャラの 2 つ。まだ選んでいなければまめドラゴンで起動する
- 絵は Codex ペット互換のスプライトシート（8 列、行ごとに 1 ポーズ）。状態ごとの静止画からも作れる（後述）

予定（未実装）: 話しかけると声で返す（AivisSpeech・天深シノ）、声で知らせる、チャットの送信、音声入力、呼びかけで音声入力を始める。設定画面とバーにこれらの項目はあるが、いまは設定を保存するだけで動かない（チャットは「送信は Phase 3 で使えるようになります」と返す）。

## 使い方

Rust（rustup）と VS 2022 の C++ ツールが要る。

```bash
cargo build --workspace --all-targets
./target/debug/pet-app.exe
```

トレイのアイコンから「設定」「位置をリセット」「終了」。二重起動はしない。

| その他の引数 | 内容 |
|---|---|
| `--pet <dir>` | `pet.json` とスプライトシートのあるフォルダを使う（既定は設定画面で選んだペット） |
| `--open-settings` | 起動してすぐ設定画面を開く |
| `--probe` | ディスプレイ構成をログと標準出力に書いて終了 |

### Claude Code とつなぐ

hooks を settings.json に登録する。`pet-hook.exe` は `pet-app.exe` と同じフォルダに置いておく（`--workspace` でビルドすれば同じフォルダに出る）。

```bash
./target/debug/pet-app.exe --install-hooks                       # 既定は ~/.claude/settings.json
./target/debug/pet-app.exe --install-hooks --settings <path>     # 別の settings.json に登録
./target/debug/pet-app.exe --uninstall-hooks                     # 登録を外す
```

- 登録する項目には `--pet-claude-managed` の印が付き、何度実行しても 1 組だけになる。印の無い既存の項目には触らない
- 書き込む前に元のファイルを `<名前>.pet-claude-backup-<時刻>` に複製する。JSON として読めないファイルは書き換えない

### ペットを足す

ペットは 1 つのフォルダ（`pet.json` とスプライトシート）。`%LOCALAPPDATA%\pet-claude\pets\<id>\` に置くと、次に設定画面を開いたときに一覧へ出る。

状態ごとの静止画（ファイル名に `idle` `walk` `working` `attention` `happy` `confused` `sleeping` `dragged` を含む PNG か WebP）があれば、次のコマンドでペットのフォルダを作れる。足りない状態は近い状態の絵で埋める。

```bash
cargo run --release -p pet-core --example build_pet -- <静止画のフォルダ> <出力フォルダ> <id> <表示名>
```

まめドラゴン（`assets/pets/mame-dragon/`）はこのコマンドで作った。1 コマは 384×416（`pet.json` の `cellScale: 2`）で、表示倍率 200% の画面でも引き伸ばさずに済む。

## 技術の詳細

- Rust のワークスペース。ペット本体は Win32 の layered window（透明な画素のクリックは OS が背後へ通す）、トレイとバー・メニュー・設定・吹き出しの UI は Tauri 2。1 プロセスでトレイ常駐。UI の WebView は使うときだけ作り、使い終わったら破棄する
- Claude Code hooks → `pet-hook.exe` → loopback HTTP（token 付き）→ アプリ。hook はどんな失敗でも exit 0 で、アプリが居なくても Claude Code を止めない。承認の答えが無いときは Claude Code のいつもの流れに戻す
- 設定と、選んだペットと、ペットの位置は `%LOCALAPPDATA%\pet-claude\state.json` に保存する
- ペットの絵は、読み込んだらすぐ表示に要る解像度まで縮めて持つ。まめドラゴンは展開すると 56MB あるが、既定の大きさで常駐の専有メモリは 22MB に収まった。行の有無とコマ数はシートの透明度から数え、空の行は代わりのポーズで動かす

| クレート | 役割 |
|---|---|
| `crates/pet-core` | OS に触らない中心ロジック（行動の状態機械・スプライト規約・配置・hook 入力の解析・IPC） |
| `crates/pet-app` | アプリ本体（ペットの窓・トレイ・UI・hooks の受け口・hooks の登録） |
| `crates/pet-hook` | Claude Code の hook から起動される小さな exe |

`ui/` が UI の HTML、`assets/pets/` がスプライトシート、`scripts/verify/` が実機確認のスクリプト。品質ゲートは `scripts/gate.sh`（単独 CR の検査 → `cargo fmt --check` → `cargo clippy -D warnings` → `cargo test`）。

計画: [docs/roadmap.md](docs/roadmap.md)。要件: [docs/product-requirements.md](docs/product-requirements.md)。設計: [docs/architecture.md](docs/architecture.md)。UI の見た目: [DESIGN.md](DESIGN.md)。参考にした OSS の解析: [OpenPets](docs/openpets-analysis.md)、[embed-pet](docs/embed-pet-analysis.md)

## ライセンス

MIT（[LICENSE](LICENSE)）
