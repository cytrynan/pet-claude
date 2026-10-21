#!/usr/bin/env bash
# 吹き出し（承認のカード）を撮る。クリックはしない（design.md「検証」4）。
#   短いコマンド → 「許可」あり / 100 字（折り返して全文）→ 「許可」あり / 長いコマンド（切り詰め）→
#   「許可」なし＋注記 / status → 固定文言「できたよ」
# 使い方: bash scripts/verify/shot-bubble.sh <出力フォルダ>
# 前提: pet-app が起動していないこと
set -u
out=${1:?出力フォルダを指定する}
repo=$(cd "$(dirname "$0")/../.." && pwd)
app="$repo/target/debug/pet-app.exe"
hook="$repo/target/debug/pet-hook.exe"
winctl="$repo/target/debug/examples/winctl.exe"
mkdir -p "$out"

long=$(printf 'x%.0s' $(seq 1 150))
wrap=$(printf 'y%.0s' $(seq 1 95))
for case in short wrap long status; do
  data="$out/data-$case"
  rm -rf "$data"; mkdir -p "$data"
  PET_CLAUDE_DATA_DIR=$(cygpath -w "$data") "$app" >/dev/null 2>&1 &
  for _ in $(seq 1 50); do [ -f "$data/runtime/ipc.json" ] && break; sleep 0.2; done
  cmd="git status"; [ "$case" = long ] && cmd="echo $long"; [ "$case" = wrap ] && cmd="echo $wrap"
  if [ "$case" = status ]; then
    printf '{"hook_event_name":"Stop","session_id":"shot"}' | PET_CLAUDE_DATA_DIR=$(cygpath -w "$data") "$hook"
    sleep 2   # 3 秒で消えるので、出ている間に撮る
  else
    printf '{"hook_event_name":"PermissionRequest","session_id":"shot","tool_name":"Bash","tool_input":{"command":"%s"}}' "$cmd" |
      PET_CLAUDE_DATA_DIR=$(cygpath -w "$data") PET_PERMISSION_WAIT_SECS=20 "$hook" >/dev/null &
    sleep 4   # 吹き出しの WebView2 を作って描くまで
  fi
  rect=$("$winctl" find | sed -n 's/^rect=(\(-\?[0-9]*\),\(-\?[0-9]*\))-(\(-\?[0-9]*\),\(-\?[0-9]*\)).*/\1 \2 \3 \4/p')
  read -r l t r b <<<"$rect"
  cx=$(( (l + r) / 2 ))
  # ペットの左上 800x(ペット+300)。右端のペットでは吹き出しが作業領域に収まるよう左へ寄る
  "$winctl" shot $((cx - 700)) $((t - 300)) 800 $((b - t + 300)) "$(cygpath -w "$out/bubble-$case.png")"
  taskkill //IM pet-hook.exe //F >/dev/null 2>&1
  taskkill //IM pet-app.exe //F >/dev/null 2>&1
  sleep 1
  grep -h "permission requested" "$data/logs/pet.log"
done
