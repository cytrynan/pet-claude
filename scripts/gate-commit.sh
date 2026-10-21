#!/usr/bin/env bash
# ゲートが通った時だけ commit する（「ゲート付き commit」を機構にする）。
# 使い方: bash scripts/gate-commit.sh <commit-message-file> [パス...]
#   パスを付けたら、そのパスだけを add して commit する（別のセッションが同じ作業ツリーで書きかけの
#   ファイルを巻き込まないため。2026-10-21）。付けなければ従来どおり全部（git add -A）
# 背景: 「ゲート → commit」を手で && で連鎖すると、echo/grep を挟んだ時に起点がずれて
# ゲート失敗のまま commit が通る（実測 4 回）。ゲートと commit を 1 本のスクリプトにする。
# 直打ちの commit は PreToolUse フック（.claude/scripts/guard-gated-commit.mjs）が止める。
set -euo pipefail
cd "$(dirname "$0")/.."
[ -f "${1:-}" ] || { echo "usage: bash scripts/gate-commit.sh <commit-message-file> [paths...]" >&2; exit 2; }
msg=$1
shift
bash scripts/gate.sh
if [ $# -gt 0 ]; then
  git add -A -- "$@"
  # 指定の外で既にステージされていたものも入れない
  git commit -F "$msg" -- "$@"
else
  git add -A
  git commit -F "$msg"
fi
git log --oneline -1
