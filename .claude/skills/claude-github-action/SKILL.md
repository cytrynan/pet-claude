---
name: claude-github-action
description: |
  GitHub リポジトリに公式の claude-code-action（anthropics/claude-code-action@v1）を入れる。
  入るのは 2 本: PR・issue で `@claude` と書くと答える／直す `claude.yml` と、PR を開いたら自動で
  レビューする `claude-review.yml`。既存リポへの後付けに使う（テンプレ由来の新規リポは /init-extras が同じ物を置く）。
  次のような時に使う:「claude-code-action を入れて」「PR で @claude を使いたい」「PR を自動レビューさせたい」
  「GitHub Actions に Claude を入れる」。CI（lint/typecheck/test）そのものの導入には使わない（/init-extras）。
---

# claude-code-action の導入

## 最初に知っておくこと（重要）

- **ワークフローは既定ブランチ（main）に入って初めて動く**。action は GitHub App のトークン交換時に
  「既定ブランチに同じワークフローがあるか」を検査し、無ければ `workflow_not_found_on_default_branch` で止まる
  （`src/github/token.ts`）。**導入の PR 自体ではレビューは走らない**。main へ入れてから試す。
- **人にしかできない作業が 2 つある**（Claude は代行しない）:
  1. GitHub App のインストール: https://github.com/apps/claude → Configure → 対象リポを選ぶ（権限付与のため）
  2. Secret `CLAUDE_CODE_OAUTH_TOKEN` の登録（トークンの入力のため）。`claude setup-token` で発行し、
     `scripts/set-oauth-secret.ps1 -Repos owner/repo1,owner/repo2` で複数リポへまとめて入れる
- 消費は**トークン発行元アカウントの利用枠**から引かれる。自動レビューは opened / ready_for_review / reopened
  だけで回し、push のたびには回さない（再レビューは `@claude 再レビューして`）。

## 手順

1. 対象リポを確認する: `gh repo view <owner/repo> --json name,visibility,defaultBranchRef`。
   既に `.github/workflows/claude*.yml` があれば中身を見せて上書きの可否を確認する。
2. `assets/claude.yml` と `assets/claude-review.yml` を `.github/workflows/` へ同名でコピーする。
   - Claude に test/lint を走らせたいときだけ、`claude.yml` の `--allowedTools` をスタックのコマンドに直して有効にする。
   - public リポで外部の人がコメントしうるなら、そのまま（write 権限者だけが起動できる）。`allowed_non_write_users` は足さない。
3. main へ commit・push する（一人開発のリポは直接 main でよい。PR 運用のリポは PR を作り、マージを人に任せる）。
4. 準備状況を確かめる:
   - Secret: `gh secret list -R <owner/repo>` に `CLAUDE_CODE_OAUTH_TOKEN` があるか
   - App: Claude 側の gh トークンでは一覧できない（`/user/installations` は 403）。初回の実行結果で判断する
5. 動作確認（人の準備が済んでから）: そのリポに issue を立てて本文に `@claude このリポの README を 3 行で要約して`
   と書く → `gh run list -R <owner/repo> --workflow claude.yml -L 1` が success、issue に返信が付けば完了。
   失敗時は `gh run view <id> -R <owner/repo> --log-failed` を読む。よくある原因:
   - `workflow_not_found_on_default_branch` → main に入っていない
   - 認証エラー → Secret 名の誤り・未登録、App 未インストール

## 報告

- 置いたファイル、commit/push の有無、人の作業 2 つの済/未、動作確認の結果を 1 行ずつ。
