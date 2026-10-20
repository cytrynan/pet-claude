# docs 更新ログ

> ドキュメント記録規約（OKF準拠。正本: `development-guidelines` スキルの `guides/recording.md`）。
> `docs/` 配下のドキュメントを作成・更新・廃止したら、同じ作業内でここに1行追記する。新しい順。
> 書式: `* **Creation|Update|Deprecation**: [タイトル](ファイル名.md) — 何をどう変えたか (アクター)`

<!-- /setup-project でドキュメントを作成したら、最初のエントリを**このコメントの下（閉じ記号の後）**に追記してください。
コメントの中に書くと Markdown の描画に出ません（実例: 「ここに追記」と読んでコメント内へ書き続け、ログ全体が見えないまま運用された）。例:
## 2026-08-01
* **Creation**: [プロダクト要求定義書](product-requirements.md) — 初版作成・ユーザー承認済み (claude/claude-sonnet-5)
-->
## 2026-10-21
* **Update**: [OpenPets 解析](openpets-analysis.md) — 「pet-claude での扱い」列を Qt/Python 前提から Rust（Win32 layered window）に直した（評価係の指摘） (kuroko/claude-opus-5-5)
* **Update**: [ロードマップ](roadmap.md) — Phase 0 の W11 を観測値（1.6 秒以内）に、PRD §7 の受け入れ条件に未達であることを明記（評価係の指摘） (kuroko/claude-opus-5-5)
* **Update**: [技術仕様書](architecture.md) — 技術を Python + PySide6 から Rust（Win32 layered window ＋ Tauri 2）へ変更。Phase 0 の実測結果を §3 に反映 (kuroko/claude-opus-5-5)
* **Update**: [ロードマップ](roadmap.md) — Phase 0 を実施済みに。Phase 2〜3 の実装言語を Rust に合わせて修正 (kuroko/claude-opus-5-5)
* **Update**: [プロダクト要求定義書](product-requirements.md) — 受け入れ条件のテスト手段を `cargo test` に (kuroko/claude-opus-5-5)
* **Creation**: [初期要件](ideas/initial-requirements.md) — K の指示（2026-10-21）を整理。状態→動きの表を含む (kuroko/claude-fable-5-1)
* **Creation**: [OpenPets 解析](openpets-analysis.md) — 転用する設計 9 点・転用しない設計 7 点・hook 対応表・シート仕様 (kuroko/claude-fable-5-1)
* **Creation**: [プロダクト要求定義書](product-requirements.md) — 初版。未決 U1〜U3 は K 確認待ち（未承認） (kuroko/claude-fable-5-1)
* **Creation**: [技術仕様書](architecture.md) — Python 3.12 + PySide6 を採用。窓・行動・hooks・声・IPC の設計 (kuroko/claude-fable-5-1)
* **Creation**: [ロードマップ](roadmap.md) — Phase 0〜4 (kuroko/claude-fable-5-1)
* **Creation**: [Phase 0 チェックリスト](checklists/phase0-window.md) — 窓の実測項目 W1〜W12 (kuroko/claude-fable-5-1)
- 2026-10-20 extras/github/workflows: ci-node.yml を改訂（paths-ignore・concurrency・permissions・TZ・生成物の注記・本番と同じ Node）、pnpm 版 ci-node-pnpm.yml を追加、init-extras の表に反映
- 2026-10-20 CLAUDE.md: /init-extras の CI を必須にし、検証の原則に「GitHub Actions でも回す」を追記
- 2026-10-20 extras/github/workflows: 起動時 env 検証向けのダミー値の注記を ci-node / ci-node-pnpm に追加
