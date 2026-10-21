#!/usr/bin/env bash
# カーソルがペットに乗ったらメニューが出て、離れたら閉じるかを確かめる（K 指示 2026-10-21）。
# 本物のカーソルを約 2 秒動かし、最後に元の位置へ戻す。**K が PC を使っていない時か、K の了承を得て流す**。
# クリックはしない（押すのはカーソルの移動だけ）。
#
# 使い方: bash scripts/verify/hover-menu.sh <出力フォルダ>
# 前提: pet-app が起動していないこと
set -u
out=${1:?出力フォルダを指定する}
repo=$(cd "$(dirname "$0")/../.." && pwd)
app="$repo/target/debug/pet-app.exe"
winctl="$repo/target/debug/examples/winctl.exe"
data="$out/data"
rm -rf "$data"; mkdir -p "$data"

PET_CLAUDE_DATA_DIR=$(cygpath -w "$data") "$app" >/dev/null 2>&1 &
for _ in $(seq 1 50); do [ -f "$data/runtime/ipc.json" ] && break; sleep 0.2; done
sleep 1
read -r _ cx0 cy0 <<<"$("$winctl" cursor)"
step() { echo "$(date +%H:%M:%S.%3N) $*" >> "$out/steps.txt"; }
: > "$out/steps.txt"
step "cursor was $cx0 $cy0"
rect=$("$winctl" find | sed -n 's/^rect=(\(-\?[0-9]*\),\(-\?[0-9]*\))-(\(-\?[0-9]*\),\(-\?[0-9]*\)).*/\1 \2 \3 \4/p')
read -r l t r b <<<"$rect"
px=$(( (l + r) / 2 )); py=$(( t + (b - t) * 6 / 10 ))   # 胴体のあたり（不透明な画素）

# 1. 乗る → 200ms 待ってメニュー
"$winctl" hover "$px" "$py" >/dev/null
"$winctl" hover $((px + 2)) "$py" >/dev/null
step "on pet $px $py"
# 初回はメニューの窓を作るので出るまで約 0.7 秒かかる（2026-10-21 実測）
sleep 1.5
step "shot shown"
"$winctl" shot $((l - 400)) $((t - 100)) $((r - l + 800)) $((b - t + 200)) "$(cygpath -w "$out/hover-shown.png")" >/dev/null
# 2. ペットの左へ大きく離れる → 400ms 後に閉じる
"$winctl" hover $((l - 300)) "$py" >/dev/null
"$winctl" hover $((l - 302)) "$py" >/dev/null
step "left to $((l - 300)) $py"
sleep 1.5
step "shot left"
# 撮ってからカーソルを戻す（元の位置がペットの上だと、戻した時点でまたメニューが出る）
"$winctl" shot $((l - 400)) $((t - 100)) $((r - l + 800)) $((b - t + 200)) "$(cygpath -w "$out/hover-left.png")" >/dev/null
"$winctl" hover "$cx0" "$cy0" >/dev/null   # カーソルを元へ戻す
step "restored"
taskkill //IM pet-app.exe //F >/dev/null 2>&1
sleep 0.5

log="$data/logs/pet.log"
shown=$(grep -c "menu on hover" "$log"); closed=$(grep -c "hover menu: left -> close" "$log")
[ "$shown" -ge 1 ] && r1=PASS || r1=FAIL
[ "$closed" -ge 1 ] && r2=PASS || r2=FAIL
echo "$r1 乗ったらメニューが出た（menu on hover ×$shown）"
echo "$r2 離れたら閉じた（left -> close ×$closed）"
[ "$r1$r2" = PASSPASS ]
