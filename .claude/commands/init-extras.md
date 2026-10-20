---
description: 開発開始時に便利な追加ファイル群 (editorconfig / .env / CI / GitHub テンプレ等) を配置
---

# 追加ファイル群の配置

このコマンドは、開発開始時にあると便利な以下のファイル群を一括で配置します。

- `.editorconfig`
- `.env.example`
- `CHANGELOG.md` (Keep a Changelog 形式)
- `CONTRIBUTING.md`
- `SECURITY.md`
- `CODE_OF_CONDUCT.md` (Contributor Covenant v2.1)
- `.github/ISSUE_TEMPLATE/bug_report.md`
- `.github/ISSUE_TEMPLATE/feature_request.md`
- `.github/pull_request_template.md`
- `.github/workflows/ci.yml` (検出スタックに応じたもの)
- `.github/dependabot.yml` (検出スタックに応じたもの)
- `.github/workflows/claude.yml` / `.github/workflows/claude-review.yml`（公式 claude-code-action。`@claude` 応答と PR 自動レビュー）

## 実行方法

```
> /init-extras
```

## 手順

### ステップ1: 技術スタックの検出

リポジトリ直下のファイルから現在のスタックを判定する。

| 検出ファイル | スタック |
|---|---|
| `package.json` | `node-ts` |
| `pyproject.toml` | `python` |
| `go.mod` | `go` |
| `Cargo.toml` | `rust` |
| いずれもなし | `none` |

複数該当する場合は AskUserQuestion で確認する。

### ステップ2: 共通ファイルの配置

`.claude/templates/extras/` 配下の以下のファイルを Read → プロジェクトルートに同名で Write する。既存ファイルがある場合は上書き確認を取る。

- `.editorconfig`
- `.env.example`
- `CHANGELOG.md`
- `CONTRIBUTING.md`
- `SECURITY.md`
- `CODE_OF_CONDUCT.md`
- `.github/ISSUE_TEMPLATE/bug_report.md`
- `.github/ISSUE_TEMPLATE/feature_request.md`
- `.github/pull_request_template.md`
- `github/workflows/claude.yml` → `.github/workflows/claude.yml`、`github/workflows/claude-review.yml` → `.github/workflows/claude-review.yml`（スタック非依存。`none` でも置く）

### ステップ3: スタック対応ファイルの配置

検出スタックに応じて、以下を配置する。

| スタック | CI ワークフロー | Dependabot |
|---|---|---|
| node-ts | `.claude/templates/extras/github/workflows/ci-node.yml` → `.github/workflows/ci.yml`（`pnpm-lock.yaml` があれば `ci-node-pnpm.yml` を使う。`node-version` は本番と同じメジャーに直す） | `.claude/templates/extras/github/dependabot-node.yml` → `.github/dependabot.yml` |
| python | `.claude/templates/extras/github/workflows/ci-python.yml` → `.github/workflows/ci.yml` | `.claude/templates/extras/github/dependabot-python.yml` → `.github/dependabot.yml` |
| go | `.claude/templates/extras/github/workflows/ci-go.yml` → `.github/workflows/ci.yml` | `.claude/templates/extras/github/dependabot-go.yml` → `.github/dependabot.yml` |
| rust | `.claude/templates/extras/github/workflows/ci-rust.yml` → `.github/workflows/ci.yml` | `.claude/templates/extras/github/dependabot-rust.yml` → `.github/dependabot.yml` |
| none | 配置しない (CI のテンプレが特定できないため、後で追加することを案内) | 配置しない |

### ステップ3.5: claude-code-action の人の作業を案内する

`claude.yml` / `claude-review.yml` は **main に入って初めて動く**（導入 PR 自体ではレビューは走らない）。次の 2 つは人にしかできないので、完了メッセージで案内する（詳細は `claude-github-action` スキル）:

1. GitHub App のインストール: https://github.com/apps/claude → 対象リポを選ぶ
2. Secret `CLAUDE_CODE_OAUTH_TOKEN` の登録: `claude setup-token` で発行 → `.claude/skills/claude-github-action/scripts/set-oauth-secret.ps1 -Repos owner/repo`（API 従量課金なら両ファイルの `claude_code_oauth_token` 行を `anthropic_api_key: ${{ secrets.ANTHROPIC_API_KEY }}` に直す）

### ステップ4: プレースホルダーの解決

`SECURITY.md` と `CODE_OF_CONDUCT.md` には連絡先プレースホルダーがある。対話的に取得して Edit ツールで置換する。

質問:
- セキュリティ連絡先メールアドレス: 例 `security@example.com`
- 行動規範の連絡先メールアドレス: 例 `conduct@example.com` (空欄ならセキュリティ連絡先と同じものを使う)

置換:
- `SECURITY.md` 内の `{{SECURITY_CONTACT_EMAIL}}` → 入力値
- `CODE_OF_CONDUCT.md` 内の `{{CONTACT_EMAIL}}` → 入力値

### ステップ5: Conventional Commits + commitlint (Node.js スタック時のみ)

スタックが `node-ts` の場合、Husky が既に導入されているので、commitlint を追加する案内をする (実際の依存追加はユーザーに委ねる)。

```
Conventional Commits の自動チェックを追加するには、以下を実行してください:

  npm install --save-dev @commitlint/cli @commitlint/config-conventional
  echo "export default { extends: ['@commitlint/config-conventional'] };" > commitlint.config.js
  echo 'npx --no-install commitlint --edit "$1"' > .husky/commit-msg
```

## 完了条件

- 共通ファイルがすべて配置されている
- 検出スタックに応じた CI と Dependabot が配置されている
- `claude.yml` / `claude-review.yml` が配置され、人の作業 2 つ（App・Secret）を案内した
- セキュリティ/行動規範のメールアドレスプレースホルダーが置換されている

完了時のメッセージ:
```
「追加ファイル群の配置が完了しました!

配置した内容:
✅ .editorconfig
✅ .env.example
✅ CHANGELOG.md
✅ CONTRIBUTING.md
✅ SECURITY.md
✅ CODE_OF_CONDUCT.md
✅ .github/ISSUE_TEMPLATE/ (bug_report / feature_request)
✅ .github/pull_request_template.md
✅ .github/workflows/ci.yml ({stack} 向け)
✅ .github/dependabot.yml ({stack} 向け)
✅ .github/workflows/claude.yml / claude-review.yml (claude-code-action)

次のステップ:
- claude-code-action を動かすには: ① https://github.com/apps/claude で App を入れる ② `claude setup-token` → Secret `CLAUDE_CODE_OAUTH_TOKEN` を登録（main に入ってから動く）
- /setup-project を実行して永続ドキュメントを作成
」
```
