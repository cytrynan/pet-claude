# 出自と改変

- 生成元: `npm install -g @playwright/cli` (0.1.22) → `playwright-cli install --skills claude --global` が出力した SKILL.md と references/ をそのまま収録（Microsoft Playwright・Apache-2.0）。
- **改変は1点だけ: SKILL.md フロントマターの `allowed-tools: Bash(playwright-cli:*) ...` 行を削除した。**
  - 理由: この行があると、スキルが読み込まれたセッションでは `click` / `fill` / `eval` / `run-code` を含む全コマンドが確認なしで許可される。
    `claude -p --permission-mode acceptEdits --allowedTools "Skill,Read"`（Bash 不許可）でも `playwright-cli` が実行できることを 2026-10-13 に実測した＝`--allowedTools` で組んだ無人セッションの権限設計をスキル経由で素通りする。
  - CLI を再インストール・`install --skills` し直すと行が戻るので、そのたびに削除し直すこと。
- 前提: `playwright-cli` がグローバルに入っていること（Node 18+）。入っていなければこのスキルは使わない。
- Playwright MCP との住み分け: ツール単位の allowlist で仕切りたい無人セッションは MCP（`mcp__playwright__browser_click` 等を個別に許可できる）、それ以外は CLI（スナップショットをファイルに出しコンテキストに流さない）。
