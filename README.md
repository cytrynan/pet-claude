# claude-code-template

Claude Code を使ったスペック駆動開発のための **言語非依存の汎用テンプレート** です。

クローン後、対話的なスラッシュコマンドで技術スタック・プロジェクト情報を初期化し、開発を開始できます。

---

## 要点（まずここだけ）

**1. これは何か** — 「思いつきで書き始めない」ための足場。仕様を先に文書化し、作業単位ごとに計画を切り、
サブエージェントに委任して、最後に司令塔が自分で検証する。その型（ドキュメント構成・スキル・エージェント定義）を一式で持ち込む。

**2. 始め方（この順に4つ）**

```
/init-stack      → 技術スタックを選んで設定ファイル一式を生成
/init-project    → プロジェクト名・著者・remote を設定
/setup-project   → docs/ の永続ドキュメント6本を作成
/add-feature X   → 機能を実装（計画 → 実装 → 検証）
```

**3. ドキュメントは2層** — 混ぜない。

| 層 | 場所 | 役割 |
|---|---|---|
| 恒久仕様 | `docs/`（UIがあればルートの `DESIGN.md`） | 「何を作るか」。頻繁に変わらない北極星 |
| 作業単位 | `.steering/<YYYYMMDD>-<タスク名>/` | 「今回何をするか」。requirements / design / tasklist。作業ごとに新規作成し履歴として残す |

**4. 進捗は tasklist.md と GitHub issue の二本立て** — `tasklist.md` を作ったら**実装着手前に**issueへ起票する
（親1件＋子N件）。以後、着手・完了のたびに両方をその場で更新する。
これで「Claude Code に次のタスクを聞く」「issue を見て `#12 やって` と指示する」の**どちらでも同じ作業に入れる**。
→ 詳細は後述の「タスク管理」節

**5. 守る運用は3つ**

- **司令塔＋役割別エージェント** — メインセッションは管理・統合・最終判断に徹し、実作業は委任する（基本2〜4体）
- **自己申告を信頼しない（3層ゲート）** — 実装係 → レビュー係が再実行 → 司令塔がもう一度実行
- **記録はその都度・具体例と抽象化のセットで・失敗を必ず1件** — 薄い振り返りは書かないより悪い

**6. 迷ったら** — 普通に会話で依頼すれば Claude Code が適切なスキルを選ぶ。運用の正本は
`CLAUDE.md`（規範）、`steering` スキル（作業単位の進め方）、`orchestration` スキル（委任と検証）。

---

以下は詳細です。必要な時に読んでください。

## 含まれるもの

### 対応技術スタック

`/init-stack` で以下から選択して初期化できます。

| スタック | 構成 |
|---|---|
| **Node.js + TypeScript** | TypeScript / Vitest / ESLint / Prettier / Husky |
| **Python** | uv / ruff / mypy (strict) / pytest |
| **Go** | go modules / golangci-lint / 標準 testing |
| **Rust** | cargo / clippy / rustfmt |
| **なし (言語非依存)** | 設定ファイルは配置しない汎用構成 |

### 共通の開発環境

- **Dev Container** — 環境差異のない開発環境 (Claude Code 込み)
- **`/init-extras`** で追加できる開発支援ファイル群:
  - `.editorconfig`、`.env.example`、`CHANGELOG.md`、`CONTRIBUTING.md`、`SECURITY.md`、`CODE_OF_CONDUCT.md`
  - GitHub Issue/PR テンプレート、CI ワークフロー、Dependabot 設定

### Claude Code スラッシュコマンド

| コマンド | 説明 |
|---|---|
| `/init-stack` | 技術スタックを選択して設定ファイル一式を生成 |
| `/init-project` | プロジェクト名・著者・年・remote を一括設定 |
| `/init-extras` | editorconfig / CI / GitHub テンプレ等を配置 |
| `/setup-project` | 6つの永続ドキュメントを対話的に作成 |
| `/add-feature [機能名]` | 既存パターンに従って機能を実装 |
| `/review-docs [ファイル]` | ドキュメントの詳細レビュー |
| `/archive-project` | 開発廃止時のチェックリストを実行 |

### スキル

| スキル | 説明 |
|---|---|
| `steering` | 作業計画・実装・振り返りの管理 |
| `tasklist-to-issues` | tasklist.md から GitHub issue を一括起票し、issue との食い違いを検出 |
| `orchestration` | 司令塔としての編成判断・委任の設計・受入検証・采配の記録 |
| `db-readonly-inspect` | 受け取った .env だけで他プロジェクトの PostgreSQL を読み取り専用で点検（二重ガード・証跡つき） |
| `prd-writing` | プロダクト要求定義書の作成 |
| `functional-design` | 機能設計書の作成 |
| `architecture-design` | アーキテクチャ設計書の作成 |
| `development-guidelines` | 開発ガイドラインの作成 |
| `glossary-creation` | 用語集の作成 |
| `repository-structure` | リポジトリ構造定義書の作成 |

### ディレクトリ構造 (クローン直後)

```
.
├── .claude/              # Claude Code 設定
│   ├── agents/           # サブエージェント定義
│   ├── commands/         # スラッシュコマンド (/init-stack 等)
│   ├── skills/           # スキル定義
│   └── templates/        # /init-* で展開されるテンプレ素材
│       ├── stacks/       # 言語別の設定ファイル
│       └── extras/       # 追加ファイル群 (CI / GitHub 等)
├── .devcontainer/        # Dev Container 設定 (汎用)
├── .steering/            # 作業単位のドキュメント (自動生成)
├── docs/
│   └── ideas/            # アイデアメモ (壁打ち用)
├── .gitignore
├── CLAUDE.md             # Claude Code へのプロジェクト指示
├── LICENSE
└── README.md
```

> 各ファイル・ディレクトリの詳細な説明は [STRUCTURE.md](./STRUCTURE.md) を参照してください。

## 使い方

### 1. このテンプレートをクローン

```bash
git clone <このリポジトリ> <新しいプロジェクト名>
cd <新しいプロジェクト名>
```

### 2. Dev Container で開く (推奨)

VS Code で「Reopen in Container」を選択すると、Claude Code の最新版が自動でインストールされます。

> Dev Container を使うには Docker が必要です。コンテナを使わずローカルで Claude Code を使う場合はこのステップは不要です。

### 3. 技術スタックを初期化

```
/init-stack
```

対話的に以下を行います。

- 技術スタックを選択 (Node.js+TS / Python / Go / Rust / なし)
- 選択したスタックの設定ファイル一式を生成
- `.devcontainer/devcontainer.json` に必要な feature を追記
- `.gitignore` に言語固有エントリを追記
- `CLAUDE.md` の「技術スタック」節を更新
- (任意) 続けて `/init-extras` の処理も実行

### 4. プロジェクト情報を初期化

```
/init-project
```

対話的に以下を取得し、関連ファイル内のプレースホルダーを一括置換します。

- プロジェクト名
- 説明
- 著者名
- ライセンス年
- (任意) git remote URL

### 5. (任意) 追加ファイル群を配置

ステップ3で配置しなかった場合は、ここで実行できます。

```
/init-extras
```

`.editorconfig` / `.env.example` / `CHANGELOG.md` / `CONTRIBUTING.md` / `SECURITY.md` / `CODE_OF_CONDUCT.md` / GitHub Issue/PR テンプレ / CI ワークフロー / Dependabot 設定 を配置します。

### 6. 依存関係のインストール

選択したスタックに応じて手動で実行してください。

```bash
# Node.js + TypeScript
npm install

# Python
uv sync

# Go
go mod download

# Rust
cargo fetch
```

### 7. 永続ドキュメントを作成

```
/setup-project
```

以下を対話的に作成します。

1. `docs/product-requirements.md` — PRD
2. `docs/functional-design.md` — 機能設計書
3. `docs/architecture.md` — 技術仕様書
4. `docs/repository-structure.md` — リポジトリ構造定義書
5. `docs/development-guidelines.md` — 開発ガイドライン
6. `docs/glossary.md` — 用語集

### 8. 機能を実装

```
/add-feature <機能名>
```

ステアリングファイル (`.steering/`) を作成して、計画 → 実装 → 検証を進めます。計画生成後に一度だけ承認を挟み、承認後は実装〜検証を自動で完走します。

## タスク管理

`tasklist.md`（リポジトリ内のファイル）と GitHub issue を**同じ内容の二本立て**で持ちます。

- **ファイル側**（`.steering/<unit>/tasklist.md`）— 設計と受入条件の正本。実装エージェントがこれを読む
- **issue 側** — 人間の操作面。一覧で眺める・追加する・閉じる・`#12 やって` と番号で指示する

### 起票（実装着手前に必ず）

`tasklist.md` が固まったら `tasklist-to-issues` スキルで起票します。

```powershell
# 対象を確認（変更しない）
.\.claude\skills\tasklist-to-issues\scripts\new-issues-from-tasklist.ps1 -TasklistPath ".steering\20260728-機能名\tasklist.md" -DryRun

# 起票して tasklist.md に番号を書き戻す
.\.claude\skills\tasklist-to-issues\scripts\new-issues-from-tasklist.ps1 -TasklistPath ".steering\20260728-機能名\tasklist.md"
```

| | 内容 |
|---|---|
| 親issue | `[tracking] <YYYYMMDD>-<タスク名>` / ラベル `tracking` + `task/<unit>` / 本文に子issueのチェックリスト |
| 子issue | タイトルはタスク名 / ラベル `task/<unit>` / 本文に受入条件・対象ファイル・`Part of #<親>` |
| 対応付け | `tasklist.md` の先頭 `tracking: #<親>` と各行末の ` (#<子>)`。**この行が対応表**（別ファイルを作らない） |

再実行しても二重起票しません（既に番号が付いた行・スキップ済み・コードフェンス内は対象外）。
子issueが10件を超えると停止します（issue 一覧が読めなくなると本来の目的を壊すため、タスクを階層化する）。

### 実装中の同期（その場で・まとめない）

| tasklist.md | issue |
|---|---|
| `- [ ]` 未着手 | open / assignee なし |
| `- [~]` 着手中 | open / assignee = 自分 / ラベル `in-progress` |
| `- [x]` 完了 | closed ＋ 何を変えたか1〜3行のコメント |
| `- [x] ~~名~~（理由）` スキップ | closed as **not planned** ＋ 理由コメント |

### issue 側の変更を取り込む

セッション開始時・指示を受けた時に食い違いを確認します。

```powershell
.\.claude\skills\tasklist-to-issues\scripts\check-tasklist-issues.ps1 -TasklistPath ".steering\20260728-機能名\tasklist.md"
```

issue にしか無いタスクは追記し、closed なら `[x]` にし、**issue が open なのに `[x]` の食い違いは勝手に直さずユーザーに確認**します。
状態（open/closed・タスクの増減）は issue を上流とみなし、設計と受入条件の正本は `.steering/` に置く、という分担です。

> GitHub リモートが無い / `gh` が未認証のリポジトリでは起票をスキップし、その旨を報告して `tasklist.md` だけで進めます（黙って飛ばしません）。

## 開発の進め方 (司令塔方式)

このテンプレートは、メインセッションを「**司令塔**」と位置づけます。司令塔は管理・統合・最終判断に徹し、実作業はサブエージェントへ委任します。運用の正本は `orchestration` スキルです。

- **編成の判断は「タスクの種類」ではなく「成果物の独立性」で行う** — 基本2〜4体・最大5体 (疎通経路は `n(n-1)/2` で増える)
- **役割にモデルを固定しない** — 固定すると、そのモデルがその役割で本当に強いかを検証する機会が失われる
- **委任は入力と出力の両方が有界なときだけ効く** — 出力が無界な依頼 (大量列挙・「関連ファイルを全部読んで」) は要約と捏造を誘発する
- **自己申告を信頼しない (3層ゲート)** — 実装係 → レビュー係が再実行 → 司令塔がもう一度実行。数値の食い違いがそのまま異常検知になる
- **否定の結論は裏取りしてから書く** — 「〜が無い」は1手足りないまま断定されやすい
- **体制を組んだら采配の評価係を立てる** — `orchestrator-reviewer` サブエージェント。自分の分解・割り振りは自分では見えない
- **記録は具体例と抽象化のセットで、失敗を必ず1件** — `.steering/<日付>-<タスク名>/orchestration.md` に残す

## 日常の使い方

通常は会話形式で依頼するだけで Claude Code が適切なスキルを判断します。

```
> PRD に新機能を追加して
> architecture.md のパフォーマンス要件を見直して
> /add-feature ユーザープロフィール編集
> /review-docs docs/product-requirements.md
```

## 開発廃止時

リポジトリをアーカイブする際に実行してください。

```
/archive-project
```

以下を自動で実施 / 案内します。

- 秘密情報の最終チェック (`.env`、鍵ファイル等の検出)
- `README.md` 冒頭に Archived バナーを挿入
- `.steering/` を `archive/steering-YYYYMMDD/` に移動
- 依存固定の提案 (スタックに応じた手順)
- 最終リリースタグ作成の案内
- `gh repo archive` のコマンド案内 (自動実行しません)

## カスタマイズ

### 新しい技術スタックを追加する

`.claude/templates/stacks/<新スタック名>/` に設定ファイル一式を配置し、`.claude/templates/stacks/claude-md-sections/<新スタック名>.md` を追加。`.claude/commands/init-stack.md` の選択肢にも追記してください。

### 追加ファイル群をカスタマイズ

`.claude/templates/extras/` 配下のファイルを編集すれば、`/init-extras` 実行時の出力が変わります。

## ライセンス

MIT — 詳細は [LICENSE](./LICENSE) を参照。
