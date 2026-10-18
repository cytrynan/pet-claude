# リポジトリ構造リファレンス

`claude-code-template` の全ディレクトリ・ファイルの役割を網羅した参考資料です。

このテンプレートをクローンした直後の状態を基準に説明します。`/init-stack` 等のコマンドを実行すると、追加のファイルがプロジェクトルートに展開されます。

## 凡例

各ファイルには以下のラベルを付与しています。

- 🔒 **コア** — テンプレートの動作に必須。原則編集不要
- 🛠️ **設定** — Claude Code 設定。プロジェクトに合わせて編集する
- 📝 **編集** — ユーザーが日常的に編集するもの
- 🎁 **テンプレ素材** — `/init-*` コマンドで展開される素材。原則直接編集しない
- 📦 **自動生成** — コマンド実行で自動生成・更新される

---

## 全体構造図

```
.
├── .claude/                                # Claude Code 設定とコマンド一式
│   ├── agents/                             # サブエージェント定義
│   │   ├── doc-reviewer.md
│   │   ├── implementation-validator.md
│   │   └── orchestrator-reviewer.md
│   ├── commands/                           # スラッシュコマンド (.md ファイル)
│   │   ├── add-feature.md
│   │   ├── archive-project.md
│   │   ├── init-extras.md
│   │   ├── init-project.md
│   │   ├── init-stack.md
│   │   ├── review-docs.md
│   │   └── setup-project.md
│   ├── settings.json                       # 共有 (commit) する Claude 設定
│   ├── settings.local.json                 # 個人 (.gitignore) の Claude 設定
│   ├── skills/                             # スキル定義
│   │   ├── architecture-design/
│   │   │   ├── SKILL.md
│   │   │   ├── guide.md
│   │   │   └── template.md
│   │   ├── development-guidelines/
│   │   │   ├── SKILL.md
│   │   │   ├── template.md
│   │   │   └── guides/
│   │   │       ├── implementation.md
│   │   │       ├── process.md
│   │   │       ├── verification.md
│   │   │       └── recording.md
│   │   ├── functional-design/
│   │   │   ├── SKILL.md
│   │   │   ├── guide.md
│   │   │   └── template.md
│   │   ├── glossary-creation/
│   │   │   ├── SKILL.md
│   │   │   ├── guide.md
│   │   │   └── template.md
│   │   ├── design-tokens/
│   │   │   ├── SKILL.md
│   │   │   └── template.md
│   │   ├── orchestration/
│   │   │   ├── SKILL.md
│   │   │   └── templates/
│   │   │       └── orchestration.md
│   │   ├── prd-writing/
│   │   │   ├── SKILL.md
│   │   │   └── template.md
│   │   ├── repository-structure/
│   │   │   ├── SKILL.md
│   │   │   ├── guide.md
│   │   │   └── template.md
│   │   └── steering/
│   │       ├── SKILL.md
│   │       └── templates/
│   │           ├── design.md
│   │           ├── requirements.md
│   │           └── tasklist.md
│   └── templates/                          # /init-* コマンドで展開する素材
│       ├── stacks/                         # 技術スタック別の設定ファイル
│       │   ├── claude-md-sections/         # CLAUDE.md に貼る技術スタック節
│       │   │   ├── dotnet.md                   # .NET 8 / WinForms（節のみ。stacks/dotnet/ の雛形は未整備）
│       │   │   ├── go.md
│       │   │   ├── node-ts.md
│       │   │   ├── none.md
│       │   │   ├── python.md
│       │   │   └── rust.md
│       │   ├── go/                         # Go スタック
│       │   │   ├── .golangci.yml
│       │   │   ├── go.mod
│       │   │   ├── main.go
│       │   │   └── main_test.go
│       │   ├── node-ts/                    # Node.js + TypeScript スタック
│       │   │   ├── .husky/
│       │   │   │   └── pre-commit
│       │   │   ├── .nvmrc
│       │   │   ├── .prettierignore
│       │   │   ├── .prettierrc
│       │   │   ├── eslint.config.js
│       │   │   ├── package.json
│       │   │   ├── src/
│       │   │   │   ├── example.test.ts
│       │   │   │   └── example.ts
│       │   │   ├── tsconfig.json
│       │   │   └── vitest.config.ts
│       │   ├── python/                     # Python スタック
│       │   │   ├── .python-version
│       │   │   ├── pyproject.toml
│       │   │   ├── ruff.toml
│       │   │   ├── src/
│       │   │   │   └── example.py
│       │   │   └── tests/
│       │   │       └── test_example.py
│       │   └── rust/                       # Rust スタック
│       │       ├── Cargo.toml
│       │       ├── clippy.toml
│       │       ├── rustfmt.toml
│       │       └── src/
│       │           ├── lib.rs
│       │           └── main.rs
│       └── extras/                         # /init-extras で展開する汎用ファイル
│           ├── .editorconfig
│           ├── .env.example
│           ├── CHANGELOG.md
│           ├── CODE_OF_CONDUCT.md
│           ├── CONTRIBUTING.md
│           ├── SECURITY.md
│           └── github/
│               ├── ISSUE_TEMPLATE/
│               │   ├── bug_report.md
│               │   └── feature_request.md
│               ├── dependabot-go.yml
│               ├── dependabot-node.yml
│               ├── dependabot-python.yml
│               ├── dependabot-rust.yml
│               ├── pull_request_template.md
│               └── workflows/
│                   ├── ci-go.yml
│                   ├── ci-node.yml
│                   ├── ci-python.yml
│                   └── ci-rust.yml
├── .devcontainer/
│   └── devcontainer.json                   # Dev Container 設定 (汎用ベース)
├── .gitignore                              # OS/エディタ系の汎用エントリ
├── .steering/                              # 作業単位のステアリングドキュメント
│   └── .gitkeep                            # ディレクトリ保持用
│                                           # (作業ディレクトリ <YYYYMMDD>-<task-name>/ は /add-feature 実行時に自動生成)
├── CLAUDE.md                               # Claude Code へのプロジェクト指示書
├── LICENSE                                 # MIT ライセンス
├── README.md                               # テンプレートの利用方法
├── STRUCTURE.md                            # 本ファイル (構造リファレンス)
└── docs/
    ├── log.md                              # docs/ の変更履歴 (OKF準拠記録規約)
    └── ideas/                              # 壁打ち・アイデアメモ用 (/setup-project の入力)
        └── initial-requirements.md
```

---

## ルート直下

| パス | ラベル | 説明 |
|---|---|---|
| `CLAUDE.md` | 🛠️ 設定 | Claude Code がプロジェクト開始時に自動で読み込む指示書。「技術スタック」「スペック駆動開発の基本原則」「初回セットアップフロー」「日常の使い方」等を記述。`/init-stack` で技術スタック節が書き換わる。 |
| `README.md` | 📝 編集 | テンプレートの利用方法。GitHub 等のリポジトリ正面玄関。`/init-project` でプロジェクト名が置換される。 |
| `STRUCTURE.md` | 🔒 コア | 本ファイル。全ディレクトリ・ファイルの説明と構造図。 |
| `LICENSE` | 📝 編集 | MIT ライセンス。`/init-project` で著者名・年が置換される。 |
| `.gitignore` | 🛠️ 設定 | OS/エディタ系の汎用エントリのみ含む。`/init-stack` で言語固有エントリが追記される。 |
| `DESIGN.md` | 📝 編集 | デザイン方向性・トークン・禁止事項の単一の正 (UIを持つプロジェクトのみ)。`/setup-project` ステップ7で `design-tokens` スキルにより生成 (初期は未生成)。 |

## `.devcontainer/`

| パス | ラベル | 説明 |
|---|---|---|
| `.devcontainer/devcontainer.json` | 🛠️ 設定 | Dev Container の最小構成。ベースイメージ + Claude Code feature のみ。`/init-stack` でスタック別の feature と `postCreateCommand` が追記される。`/init-project` で `name` と `workspaceFolder` が置換される。 |

## `.claude/` (Claude Code 設定一式)

### `.claude/settings.json` / `settings.local.json`

| パス | ラベル | 説明 |
|---|---|---|
| `.claude/settings.json` | 🛠️ 設定 | チーム共有する Claude Code 設定。スキル使用許可リスト等。コミット対象。 |
| `.claude/settings.local.json` | 🛠️ 設定 | 個人ローカルの Claude Code 設定。`.gitignore` でコミット対象から除外される。 |

### `.claude/agents/` — サブエージェント定義

| パス | ラベル | 説明 |
|---|---|---|
| `.claude/agents/doc-reviewer.md` | 🔒 コア | ドキュメントの詳細レビュー用サブエージェント。`/review-docs` から呼ばれる。 |
| `.claude/agents/implementation-validator.md` | 🔒 コア | 実装の品質・スペック整合性を検証するサブエージェント。品質ゲートを**再実行**して報告する（第2層）。 |
| `.claude/agents/orchestrator-reviewer.md` | 🔒 コア | **司令塔の采配そのもの**（分解・割り振り・編成数・差し戻し・受入検証）を評価するサブエージェント。エージェント体制を組んだ作業では毎回1体立てる。 |

### `.claude/commands/` — スラッシュコマンド

| パス | ラベル | 説明 |
|---|---|---|
| `.claude/commands/init-stack.md` | 🔒 コア | `/init-stack`: 技術スタック (Node-TS / Python / Go / Rust / なし) を対話選択し、`.claude/templates/stacks/<stack>/` を展開。 |
| `.claude/commands/init-project.md` | 🔒 コア | `/init-project`: プロジェクト名・著者・年・remote のプレースホルダーを対話で取得して一括置換。 |
| `.claude/commands/init-extras.md` | 🔒 コア | `/init-extras`: editorconfig/CI/GitHub テンプレ等の追加ファイル群を配置。 |
| `.claude/commands/archive-project.md` | 🔒 コア | `/archive-project`: 廃止チェックリスト (秘密情報検出・READMEバナー・.steering 移動・依存固定・タグ・`gh repo archive` 案内)。 |
| `.claude/commands/setup-project.md` | 🔒 コア | `/setup-project`: `docs/` 配下の6つの永続ドキュメントを対話的に作成。UIを持つプロジェクトではルート `DESIGN.md` も作成。 |
| `.claude/commands/add-feature.md` | 🔒 コア | `/add-feature [機能名]`: ステアリングファイルを作成して計画→実装→検証のフローを実行。 |
| `.claude/commands/review-docs.md` | 🔒 コア | `/review-docs [ファイル]`: doc-reviewer サブエージェントでドキュメント詳細レビュー。 |

### `.claude/skills/` — スキル定義

各スキルは `SKILL.md` (定義) + `template.md` (出力テンプレ) + `guide.md` (詳細ガイド、一部のみ) で構成。

| スキル | 主要ファイル | ラベル | 説明 |
|---|---|---|---|
| prd-writing | `.claude/skills/prd-writing/SKILL.md` `template.md` | 🔒 コア | プロダクト要求定義書 (PRD) 作成スキル。`/setup-project` ステップ1で使用。 |
| functional-design | `.claude/skills/functional-design/SKILL.md` `guide.md` `template.md` | 🔒 コア | 機能設計書作成スキル。 |
| architecture-design | `.claude/skills/architecture-design/SKILL.md` `guide.md` `template.md` | 🔒 コア | アーキテクチャ設計書作成スキル。 |
| repository-structure | `.claude/skills/repository-structure/SKILL.md` `guide.md` `template.md` | 🔒 コア | リポジトリ構造定義書作成スキル。 |
| development-guidelines | `.claude/skills/development-guidelines/SKILL.md` `template.md` `guides/implementation.md` `guides/process.md` `guides/verification.md` `guides/recording.md` | 🔒 コア | 開発ガイドライン作成スキル。実装・プロセス・検証・記録の4ガイドを含む。`guides/verification.md` は外部連携/無人実行の検証設計（証跡・ground truth・完了判定）。`guides/recording.md` はOKF準拠のドキュメント記録規約（フロントマター・docs/log.md・アクター規約）。 |
| glossary-creation | `.claude/skills/glossary-creation/SKILL.md` `guide.md` `template.md` | 🔒 コア | ユビキタス言語 (用語集) 作成スキル。 |
| design-tokens | `.claude/skills/design-tokens/SKILL.md` `template.md` | 🔒 コア | デザイン方向性・トークンをルート `DESIGN.md` に固定するスキル (AI slop 回避)。`/setup-project` ステップ7・UI実装前に使用。 |
| apple-design | `.claude/skills/apple-design/SKILL.md` `references/*.md` `LICENSE` `ORIGIN.md` | 🔒 コア | Apple 風デザイン (HIG＋Liquid Glass) の仕様集。外部 MIT スキル (SudewaJay/apple-design-skill) を無改変収録。方向性に Apple 風を選んだ時に使用。 |
| liquid-glass | `.claude/skills/liquid-glass/SKILL.md` `liquid-glass.js` `README.md` `demo/` `LICENSE` `ORIGIN.md` | 🔒 コア | Liquid Glass の屈折表現を任意要素へ付ける依存ゼロの JS。外部 MIT スキル (deepika-builds/liquid-glass) を無改変収録。ガラス表現は自作せずこれを使う。 |
| steering | `.claude/skills/steering/SKILL.md` `templates/{requirements,design,tasklist}.md` | 🔒 コア | 作業単位のステアリングファイル管理スキル。`/add-feature` 等から内部的に呼ばれる。3つのテンプレ (要件/設計/タスクリスト) を保持。 |
| tasklist-to-issues | `.claude/skills/tasklist-to-issues/SKILL.md` `scripts/{new-issues-from-tasklist,check-tasklist-issues}.ps1` | 🔒 コア | `tasklist.md` から GitHub issue (親1件＋子N件) を一括起票し番号を書き戻す／issue との食い違いを検出。実装着手前に使用。スクリプトは PowerShell (UTF-8 BOM付きで保持すること)。 |
| orchestration | `.claude/skills/orchestration/SKILL.md` `templates/orchestration.md` | 🔒 コア | **司令塔の運用の正本**。編成の判断 / 委任の設計 / 受入検証 (3層ゲート) / 采配の記録の4モード。`/add-feature` ステップ3.5 で呼ばれる。 |
| db-readonly-inspect | `.claude/skills/db-readonly-inspect/SKILL.md` `scripts/{lib,env-keys,inspect,ro-query,print,selftest}.js` | 🔒 コア | 受け取った `.env` だけで他プロジェクトの PostgreSQL を**読み取り専用**で点検する。生成済み Prisma Client を接続先差し替えで流用し、送信前 lint ＋ `SET TRANSACTION READ ONLY` の二重ガードと `SHOW transaction_read_only` の証跡つき。接続情報の値は出力しない。 |

### `.claude/templates/` — `/init-*` で展開するテンプレ素材

#### `stacks/` — 技術スタック別

| パス | ラベル | 展開先 |
|---|---|---|
| `stacks/node-ts/package.json` | 🎁 テンプレ素材 | `package.json` (Node-TS 選択時) |
| `stacks/node-ts/tsconfig.json` | 🎁 テンプレ素材 | `tsconfig.json` |
| `stacks/node-ts/vitest.config.ts` | 🎁 テンプレ素材 | `vitest.config.ts` |
| `stacks/node-ts/eslint.config.js` | 🎁 テンプレ素材 | `eslint.config.js` |
| `stacks/node-ts/.prettierrc` | 🎁 テンプレ素材 | `.prettierrc` |
| `stacks/node-ts/.prettierignore` | 🎁 テンプレ素材 | `.prettierignore` |
| `stacks/node-ts/.nvmrc` | 🎁 テンプレ素材 | `.nvmrc` (`22`) |
| `stacks/node-ts/.husky/pre-commit` | 🎁 テンプレ素材 | `.husky/pre-commit` |
| `stacks/node-ts/src/example.ts` | 🎁 テンプレ素材 | `src/example.ts` (四則演算のサンプル) |
| `stacks/node-ts/src/example.test.ts` | 🎁 テンプレ素材 | `src/example.test.ts` (Vitest テスト例) |
| `stacks/python/pyproject.toml` | 🎁 テンプレ素材 | `pyproject.toml` (uv + ruff + mypy + pytest) |
| `stacks/python/.python-version` | 🎁 テンプレ素材 | `.python-version` (`3.12`) |
| `stacks/python/ruff.toml` | 🎁 テンプレ素材 | `ruff.toml` |
| `stacks/python/src/example.py` | 🎁 テンプレ素材 | `src/example.py` |
| `stacks/python/tests/test_example.py` | 🎁 テンプレ素材 | `tests/test_example.py` |
| `stacks/go/go.mod` | 🎁 テンプレ素材 | `go.mod` (`module {{PROJECT_NAME}}`) |
| `stacks/go/.golangci.yml` | 🎁 テンプレ素材 | `.golangci.yml` |
| `stacks/go/main.go` | 🎁 テンプレ素材 | `main.go` |
| `stacks/go/main_test.go` | 🎁 テンプレ素材 | `main_test.go` |
| `stacks/rust/Cargo.toml` | 🎁 テンプレ素材 | `Cargo.toml` |
| `stacks/rust/rustfmt.toml` | 🎁 テンプレ素材 | `rustfmt.toml` |
| `stacks/rust/clippy.toml` | 🎁 テンプレ素材 | `clippy.toml` |
| `stacks/rust/src/main.rs` | 🎁 テンプレ素材 | `src/main.rs` |
| `stacks/rust/src/lib.rs` | 🎁 テンプレ素材 | `src/lib.rs` (テストを inline 配置) |
| `stacks/claude-md-sections/node-ts.md` | 🎁 テンプレ素材 | `CLAUDE.md` の技術スタック節 (Node-TS) |
| `stacks/claude-md-sections/python.md` | 🎁 テンプレ素材 | 同上 (Python) |
| `stacks/claude-md-sections/go.md` | 🎁 テンプレ素材 | 同上 (Go) |
| `stacks/claude-md-sections/rust.md` | 🎁 テンプレ素材 | 同上 (Rust) |
| `stacks/claude-md-sections/none.md` | 🎁 テンプレ素材 | 同上 (言語非依存) |

#### `extras/` — 追加ファイル群

| パス | ラベル | 展開先 |
|---|---|---|
| `extras/.editorconfig` | 🎁 テンプレ素材 | `.editorconfig` (エディタ間で改行/インデント統一) |
| `extras/.env.example` | 🎁 テンプレ素材 | `.env.example` (環境変数の雛形) |
| `extras/CHANGELOG.md` | 🎁 テンプレ素材 | `CHANGELOG.md` (Keep a Changelog 形式) |
| `extras/CONTRIBUTING.md` | 🎁 テンプレ素材 | `CONTRIBUTING.md` (貢献ガイド) |
| `extras/SECURITY.md` | 🎁 テンプレ素材 | `SECURITY.md` (`{{SECURITY_CONTACT_EMAIL}}` プレースホルダーあり) |
| `extras/CODE_OF_CONDUCT.md` | 🎁 テンプレ素材 | `CODE_OF_CONDUCT.md` (Contributor Covenant v2.1) |
| `extras/github/ISSUE_TEMPLATE/bug_report.md` | 🎁 テンプレ素材 | `.github/ISSUE_TEMPLATE/bug_report.md` |
| `extras/github/ISSUE_TEMPLATE/feature_request.md` | 🎁 テンプレ素材 | `.github/ISSUE_TEMPLATE/feature_request.md` |
| `extras/github/pull_request_template.md` | 🎁 テンプレ素材 | `.github/pull_request_template.md` |
| `extras/github/workflows/ci-node.yml` | 🎁 テンプレ素材 | `.github/workflows/ci.yml` (Node-TS 検出時) |
| `extras/github/workflows/ci-python.yml` | 🎁 テンプレ素材 | 同上 (Python 検出時) |
| `extras/github/workflows/ci-go.yml` | 🎁 テンプレ素材 | 同上 (Go 検出時) |
| `extras/github/workflows/ci-rust.yml` | 🎁 テンプレ素材 | 同上 (Rust 検出時) |
| `extras/github/dependabot-node.yml` | 🎁 テンプレ素材 | `.github/dependabot.yml` (Node-TS 検出時) |
| `extras/github/dependabot-python.yml` | 🎁 テンプレ素材 | 同上 (Python 検出時) |
| `extras/github/dependabot-go.yml` | 🎁 テンプレ素材 | 同上 (Go 検出時) |
| `extras/github/dependabot-rust.yml` | 🎁 テンプレ素材 | 同上 (Rust 検出時) |

## `.steering/` — 作業単位のステアリングドキュメント

| パス | ラベル | 説明 |
|---|---|---|
| `.steering/.gitkeep` | 🔒 コア | ディレクトリ保持用の空ファイル。 |
| `.steering/<YYYYMMDD>-<task-name>/requirements.md` | 📦 自動生成 | 作業の要求内容。`/add-feature` 等で自動生成。 |
| `.steering/<YYYYMMDD>-<task-name>/design.md` | 📦 自動生成 | 作業の実装アプローチ。 |
| `.steering/<YYYYMMDD>-<task-name>/tasklist.md` | 📦 自動生成 | タスクリスト + 振り返り (止まった点の記録が必須)。 |
| `.steering/<YYYYMMDD>-<task-name>/orchestration.md` | 📦 自動生成 | 采配の記録。**エージェント体制を組んだ場合のみ**作成 (司令塔単独処理では作らない)。 |

`.steering/*` はデフォルトで `.gitignore` 対象だが、必要に応じてコミットすることも可能。

## `docs/` — 永続ドキュメント

| パス | ラベル | 説明 |
|---|---|---|
| `docs/ideas/initial-requirements.md` | 📝 編集 | 壁打ち・初期アイデアメモ。`/setup-project` 実行時に PRD の入力として読み込まれる。 |
| `docs/log.md` | 📝 編集 | `docs/` の変更履歴（OKF準拠記録規約）。ドキュメントの作成・更新のたびに1行追記する。正本は `development-guidelines` スキルの `guides/recording.md`。 |
| `docs/product-requirements.md` | 📝 編集 | `/setup-project` ステップ1で作成される PRD (初期は未生成)。 |
| `docs/functional-design.md` | 📝 編集 | 同ステップ2で作成 (初期は未生成)。 |
| `docs/architecture.md` | 📝 編集 | 同ステップ3で作成 (初期は未生成)。 |
| `docs/repository-structure.md` | 📝 編集 | 同ステップ4で作成 (初期は未生成)。**※テンプレ自体の構造は本ファイル `STRUCTURE.md` を参照。** |
| `docs/development-guidelines.md` | 📝 編集 | 同ステップ5で作成 (初期は未生成)。 |
| `docs/glossary.md` | 📝 編集 | 同ステップ6で作成 (初期は未生成)。 |

---

## ライフサイクル別の操作早見表

| タイミング | 実行コマンド / 操作 | 影響範囲 |
|---|---|---|
| クローン直後 | `/init-stack` | `package.json` 等の生成、`CLAUDE.md` 技術スタック節更新、`.devcontainer` `.gitignore` 追記 |
| 続いて | `/init-project` | `package.json`、`devcontainer.json`、`LICENSE`、`README.md` 内のプレースホルダー一括置換 |
| 任意 | `/init-extras` | `.editorconfig`、`.env.example`、`.github/`、`CHANGELOG.md` 等を配置 |
| 永続ドキュメント整備 | `/setup-project` | `docs/` 配下に6つのドキュメントを作成 + UIがあればルートに `DESIGN.md` を作成 |
| 機能追加 | `/add-feature <機能名>` | `.steering/<日付>-<機能名>/` を作成 + 体制判断 + 実装 + 3層の品質ゲート + 采配の記録 |
| 廃止時 | `/archive-project` | `README.md` バナー、`.steering/` 移動、依存固定提案 |

## プレースホルダー一覧

`/init-project` および `/init-extras` で置換されるプレースホルダー:

| プレースホルダー | 解決元 | 出現箇所の例 |
|---|---|---|
| `{{PROJECT_NAME}}` | `/init-project` 対話入力 | `package.json` / `pyproject.toml` / `go.mod` / `Cargo.toml` / `devcontainer.json` / Rust `main.rs` |
| `{{PROJECT_DESCRIPTION}}` | 同上 | `package.json` / `pyproject.toml` / `Cargo.toml` |
| `{{AUTHOR}}` | 同上 | `package.json` / `pyproject.toml` / `Cargo.toml` / `LICENSE` |
| `{{YEAR}}` | 同上 (既定: 現在年) | `LICENSE` |
| `{{SECURITY_CONTACT_EMAIL}}` | `/init-extras` 対話入力 | `SECURITY.md` |
| `{{CONTACT_EMAIL}}` | 同上 | `CODE_OF_CONDUCT.md` |
