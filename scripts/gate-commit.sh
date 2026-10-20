#!/usr/bin/env bash
# ゲートが通った時だけ commit する（「ゲート付き commit」を機構にする）。
# 使い方: bash scripts/gate-commit.sh <commit-message-file>
# 背景: 「ゲート → commit」を手で && で連鎖すると、echo/grep を挟んだ時に起点がずれて
# ゲート失敗のまま commit が通る（実測 4 回）。ゲートと commit を 1 本のスクリプトにする。
# 直打ちの commit は PreToolUse フック（.claude/scripts/guard-gated-commit.mjs）が止める。
set -euo pipefail
cd "$(dirname "$0")/.."
[ -f "${1:-}" ] || { echo "usage: bash scripts/gate-commit.sh <commit-message-file>" >&2; exit 2; }
bash scripts/gate.sh
git add -A
git commit -F "$1"
git log --oneline -1
