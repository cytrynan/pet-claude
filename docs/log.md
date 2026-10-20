# docs 更新ログ

> ドキュメント記録規約（OKF準拠。正本: `development-guidelines` スキルの `guides/recording.md`）。
> `docs/` 配下のドキュメントを作成・更新・廃止したら、同じ作業内でここに1行追記する。新しい順。
> 書式: `* **Creation|Update|Deprecation**: [タイトル](ファイル名.md) — 何をどう変えたか (アクター)`

<!-- /setup-project でドキュメントを作成したら、最初のエントリを**このコメントの下（閉じ記号の後）**に追記してください。
コメントの中に書くと Markdown の描画に出ません（実例: 「ここに追記」と読んでコメント内へ書き続け、ログ全体が見えないまま運用された）。例:
## 2026-08-01
* **Creation**: [プロダクト要求定義書](product-requirements.md) — 初版作成・ユーザー承認済み (claude/claude-sonnet-5)
-->
- 2026-10-20 extras/github/workflows: ci-node.yml を改訂（paths-ignore・concurrency・permissions・TZ・生成物の注記・本番と同じ Node）、pnpm 版 ci-node-pnpm.yml を追加、init-extras の表に反映
- 2026-10-20 CLAUDE.md: /init-extras の CI を必須にし、検証の原則に「GitHub Actions でも回す」を追記
