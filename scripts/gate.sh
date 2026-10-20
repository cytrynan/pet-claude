#!/usr/bin/env bash
# 品質ゲート（commit 前に必ず通す）。パイプで exit を隠さないため pipefail を先頭に。
# 3 段: cargo fmt --check → cargo clippy（警告も拒否）→ cargo test
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace -q
echo "GATE OK"
