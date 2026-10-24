#!/usr/bin/env bash
# 品質ゲート（commit 前に必ず通す）。パイプで exit を隠さないため pipefail を先頭に。
# 5 段: CR の混入 → ウェイクワードの Python（numpy 無し・有り）→ cargo fmt --check → cargo clippy（警告も拒否）→ cargo test
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
# テキストに単独の CR が無いこと（Python 文字列の \\r が CR に化けて docs に無言で混入した実例がある
# ＝2026-10-21 評価係の指摘。CRLF は commit 時に LF へ直るので見ない）
PYTHONIOENCODING=utf-8 py -3 -I scripts/check-lone-cr.py
# ウェイクワードの道具の numpy の要らない部分（ゲート・合成する言葉）
PYTHONIOENCODING=utf-8 py -3 -I -m unittest discover -s scripts/wake -p "test_wake.py" -q
# numpy の要る部分（水増し・分け方・流し方・頭・しきい値）は声の環境の python で（py -3 には numpy が無い）
WAKE_PY="${LOCALAPPDATA:-}/pet-claude/stt/venv/Scripts/python.exe"
if [ -x "$WAKE_PY" ]; then
  PYTHONIOENCODING=utf-8 "$WAKE_PY" -I -m unittest discover -s scripts/wake -p "test_wake_np.py" -q
else
  echo "skip: test_wake_np.py（$WAKE_PY が無い。CI では走る）"
fi
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -q
echo "GATE OK"
