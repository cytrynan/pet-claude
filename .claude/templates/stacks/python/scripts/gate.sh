#!/usr/bin/env bash
# 品質ゲート（commit 前に必ず通す）。パイプで exit を隠さないため pipefail を先頭に。
# 4 段: ruff format --check → ruff check → mypy → pytest（db 含む・カバレッジ 80%）
set -euo pipefail
cd "$(dirname "$0")/.."
export PYTHONIOENCODING=utf-8
uv run ruff format --check src tests
uv run ruff check .
uv run mypy src
uv run pytest -m "db or not db" --cov-fail-under=80 -q -p no:cacheprovider
echo "GATE OK"
