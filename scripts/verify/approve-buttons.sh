#!/usr/bin/env bash
# 承認の吹き出しの「許可」「拒否」を本物のマウスで押し、その答えが pet-hook から Claude Code へ返るかを確かめる
# （Phase 2 の残り・2026-10-21）。試験用のデータフォルダで別のペットを起こし、ダミーの承認（Bash の git status）を送る。
# 本物の Claude の操作は通らない。**K の了承を得て流す**（左クリックを 1 回ずつ送る）。
#
# 使い方:
#   bash scripts/verify/approve-buttons.sh <出力フォルダ> shot          … 吹き出しを撮るだけ（ボタンの位置を測る）
#   bash scripts/verify/approve-buttons.sh <出力フォルダ> <allow|deny> <dx> <dy>
#        … ペットの窓の左上から (dx,dy) の画素を左クリックし、pet-hook の出力に答えが入るか見る。
#          ボタンの位置は吹き出しの置き方で決まるので、shot の撮影から測って渡す。
# ⚠️ 動いている pet-app（K の常駐）を止める（二重起動防止で試験用が立たないため）。最後に元の exe を起動し直す。
set -u
out=${1:?出力フォルダを指定する}
mode=${2:?shot か allow か deny}
repo=$(cd "$(dirname "$0")/../.." && pwd)
app="$repo/target/debug/pet-app.exe"
hook="$repo/target/debug/pet-hook.exe"
winctl="$repo/target/debug/examples/winctl.exe"
data="$out/data-$mode"
rm -rf "$data"; mkdir -p "$data"
step() { echo "$(date +%H:%M:%S.%3N) $*" >> "$out/steps-$mode.txt"; }
: > "$out/steps-$mode.txt"

orig=$(powershell -NoProfile -Command "(Get-Process pet-app -ErrorAction SilentlyContinue | Select-Object -First 1).Path")
powershell -NoProfile -Command "Stop-Process -Name pet-app -ErrorAction SilentlyContinue" >/dev/null 2>&1
sleep 1
restore() {
  [ -n "$orig" ] && powershell -NoProfile -Command "Start-Process -FilePath '$orig' -WindowStyle Hidden" && step "restored $orig"
}
PET_CLAUDE_DATA_DIR=$(cygpath -w "$data") "$app" >/dev/null 2>&1 &
for _ in $(seq 1 50); do [ -f "$data/runtime/ipc.json" ] && break; sleep 0.2; done
sleep 1
read -r _ cx0 cy0 <<<"$("$winctl" cursor)"
step "cursor was $cx0 $cy0"
printf '{"hook_event_name":"PermissionRequest","session_id":"buttons","tool_name":"Bash","tool_input":{"command":"git status"}}' |
  PET_CLAUDE_DATA_DIR=$(cygpath -w "$data") PET_PERMISSION_WAIT_SECS=30 "$hook" > "$out/hook-$mode.json" &
hook_pid=$!
sleep 4   # 吹き出しの WebView2 を作って描くまで（shot-bubble.sh と同じ）
rect=$("$winctl" find | sed -n 's/^rect=(\(-\?[0-9]*\),\(-\?[0-9]*\))-(\(-\?[0-9]*\),\(-\?[0-9]*\)).*/\1 \2 \3 \4/p')
read -r l t r b <<<"$rect"
step "pet rect $l $t $r $b"
# 撮る範囲はペットの左上 800 x (ペット+300)（shot-bubble.sh と同じ）
cx=$(( (l + r) / 2 ))
sx=$((cx - 700)); sy=$((t - 300))
"$winctl" shot "$sx" "$sy" 800 $((b - t + 300)) "$(cygpath -w "$out/before-$mode.png")" >/dev/null
step "shot origin $sx $sy"

if [ "$mode" != shot ]; then
  dx=${3:?dx}; dy=${4:?dy}
  "$winctl" click $((l + dx)) $((t + dy)) >/dev/null
  step "clicked $((l + dx)) $((t + dy))"
  # pet-hook は答えを受け取ったら出力して終わる。30 秒の期限より前に終われば押せている
  for _ in $(seq 1 50); do kill -0 "$hook_pid" 2>/dev/null || break; sleep 0.2; done
  step "hook finished"
  "$winctl" shot "$sx" "$sy" 800 $((b - t + 300)) "$(cygpath -w "$out/after-$mode.png")" >/dev/null
fi
"$winctl" hover "$cx0" "$cy0" >/dev/null   # カーソルを元へ戻す
step "restored"
# 他の Claude セッションの pet-hook（本物の承認待ち）を巻き込まないよう、自分の分だけ止める
kill "$hook_pid" 2>/dev/null
taskkill //IM pet-app.exe //F >/dev/null 2>&1
sleep 0.5
restore
[ "$mode" = shot ] && { echo "撮影: $out/before-shot.png（原点 $sx,$sy・ペット左上 $l,$t）"; exit 0; }

echo "pet-hook の出力: $(cat "$out/hook-$mode.json")"
want="\"behavior\": *\"$mode\""
grep -Eq "$want" "$out/hook-$mode.json" && { echo "PASS $mode を押したら $want が返った"; exit 0; }
echo "FAIL $mode を押したのに $want が返らなかった"; exit 1
