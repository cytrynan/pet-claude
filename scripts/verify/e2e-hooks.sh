#!/usr/bin/env bash
# Phase 2 の端から端まで（design.md「検証」3）。K の入力を使わない。
#
# 作業用フォルダの .claude/settings.json だけに hooks を登録し、そこで `claude -p --model haiku` に
# 「ファイルを作るコマンド」を頼む。-p では承認の画面を出せないので PermissionRequest の hook が答えを決める。
# アプリはデバッグビルドの試験用 PET_AUTO_DECIDE で、吹き出しのボタンと同じ関数を 1.5 秒後に呼ぶ。
#
#   allow → ファイルができる / deny → できない / ask（hook は {} を返す）→ できない（-p では拒否扱い）
#
# 使い方: bash scripts/verify/e2e-hooks.sh <作業用の空フォルダ>
# 前提: pet-app が起動していないこと（二重起動防止で試験用のアプリが立ち上がらない）
set -u
work=${1:?作業用フォルダを指定する}
repo=$(cd "$(dirname "$0")/../.." && pwd)
app="$repo/target/debug/pet-app.exe"
hook="$repo/target/debug/pet-hook.exe"
[ -x "$app" ] && [ -x "$hook" ] || { echo "先に cargo build -p pet-app -p pet-hook"; exit 2; }
if tasklist //FI "IMAGENAME eq pet-app.exe" | grep -q pet-app.exe; then
  echo "pet-app が起動している。止めてから実行する"; exit 2
fi

fail=0
for mode in allow deny ask; do
  proj="$work/$mode/proj"
  data="$work/$mode/data"
  rm -rf "$work/$mode"
  mkdir -p "$proj/.claude" "$data"
  "$app" --install-hooks --settings "$(cygpath -w "$proj/.claude/settings.json")" >/dev/null
  # K のグローバル設定が Bash を許可していると承認の要求が起きない。ask は allow より強いので、この 1 コマンドだけ確認対象にする
  py -3 -I "$repo/scripts/verify/e2e-ask-rule.py" "$(cygpath -w "$proj/.claude/settings.json")"

  PET_TRACE=1 PET_AUTO_DECIDE=$mode PET_CLAUDE_DATA_DIR=$(cygpath -w "$data") "$app" >/dev/null 2>&1 &
  app_pid=$!
  for _ in $(seq 1 50); do [ -f "$data/runtime/ipc.json" ] && break; sleep 0.2; done
  if [ ! -f "$data/runtime/ipc.json" ]; then
    echo "[$mode] アプリの受け口が開かない"; kill "$app_pid" 2>/dev/null; fail=1; continue
  fi

  start=$(date +%s)
  out=$(cd "$proj" && PET_CLAUDE_DATA_DIR=$(cygpath -w "$data") timeout 180 claude -p --model haiku --permission-mode default \
    "Run exactly this Bash command and nothing else: echo pet-claude-e2e > e2e-marker.txt" 2>&1 </dev/null)
  took=$(( $(date +%s) - start ))
  sleep 2   # 非同期の hook（Stop など）がアプリに届くのを待つ
  taskkill //PID "$(tasklist //FI "IMAGENAME eq pet-app.exe" //FO CSV //NH | head -1 | cut -d, -f2 | tr -d '"')" //F >/dev/null 2>&1
  wait "$app_pid" 2>/dev/null

  log="$data/logs/pet.log"
  made=no; [ -f "$proj/e2e-marker.txt" ] && made=yes
  decided=$(grep -c "permission .* state=$mode (accepted=true)" "$log" 2>/dev/null)
  events=$(grep -c "permission requested" "$log" 2>/dev/null)
  kinds=$(grep -o "wire event kind=[a-z_]*" "$log" 2>/dev/null | cut -d= -f2 | sort | uniq -c | tr -s ' \n' ' ')
  expect=no; [ "$mode" = allow ] && expect=yes
  verdict=PASS
  { [ "$made" = "$expect" ] && [ "${decided:-0}" -ge 1 ]; } || { verdict=FAIL; fail=1; }
  echo "[$mode] $verdict marker=$made (期待 $expect) 承認の要求=$events 答え=$decided 所要=${took}s 届いた出来事:${kinds}"
  echo "  claude の出力: $(echo "$out" | tr '\n' ' ' | cut -c1-200)"
done
exit $fail
