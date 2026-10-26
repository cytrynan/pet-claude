#!/usr/bin/env bash
# 品質ゲート（commit 前に必ず通す）。パイプで exit を隠さないため pipefail を先頭に。
# 5 段: CR の混入 → 画面のスクリプトの構文 → cargo fmt --check → cargo clippy（警告も拒否）→ cargo test
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
# テキストに単独の CR が無いこと（Python 文字列の \\r が CR に化けて docs に無言で混入した実例がある
# ＝2026-10-21 評価係の指摘。CRLF は commit 時に LF へ直るので見ない）
PYTHONIOENCODING=utf-8 py -3 -I scripts/check-lone-cr.py
# 画面（ui/*.html の script・ui/*.js）の構文。Rust のテストは画面のスクリプトを通らないので別に見る（2026-10-22 chat.html の SyntaxError）
node scripts/check-ui-js.mjs
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -q
echo "GATE OK"
