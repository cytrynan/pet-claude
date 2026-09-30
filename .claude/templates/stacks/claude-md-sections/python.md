## 技術スタック

- 開発環境: devcontainer
- Python 3.12+
- パッケージ管理: uv
- Lint + Format: ruff
- 型チェック: mypy (strict)
- テストフレームワーク: pytest (+ pytest-cov)

## 主要コマンド

```bash
uv sync               # 依存関係を解決・インストール
uv run ruff check .   # Lint
uv run ruff format .  # Format
uv run mypy src       # 型チェック
uv run pytest         # テスト実行 (カバレッジ込み)
bash scripts/gate.sh  # 品質ゲート一式（format/check → mypy → pytest・カバレッジ 80%）。PowerShell は pwsh -File scripts/gate.ps1
bash scripts/gate-commit.sh <msg-file>  # ゲートが通った時だけ add→commit（commit はこれだけを使う）
```

commit の直打ちは PreToolUse フック `.claude/scripts/guard-gated-commit.mjs` が deny する（このプロジェクト配下だけ。他リポジトリへの commit は通るが、対象を変数で書くと解決できないので止まる＝リテラルのパスで書く。Bash のコマンド文字列に「区切りの直後の git commit」を含む文面があると止まるので、そういう文面は Write でファイルに書いてから実行する）。故障注入: `node .claude/scripts/tests/test-guard-gated-commit.mjs`。
