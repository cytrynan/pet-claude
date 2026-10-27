#!/usr/bin/env bash
# ペットの切り替え（K 指示 2026-10-21）を、本物のマウスを使わずに確かめる。
#   1. まだ何も選んでいなければ、フォルダの最初のペット（まめドラゴン）で起動する
#   2. 設定画面に「ペット」の行が出る（--open-settings で開いて撮る）
#   3. 設定画面と同じ関数で仮キャラへ切り替わり、state.json に残る（PET_TEST_SWITCH_PET）
#   4. 切り替えの前後のメモリ（子孫プロセス込みの専有）
#   rapid を付けると「仮キャラを選んで、読み込み中にまめドラゴンを選び直す」を試す（最後に選んだまめドラゴンの
#   まま残り、古い読み込みが上書きしないこと）
# 使い方: bash scripts/verify/pet-switch.sh <出力フォルダ> [rapid]
#   環境変数 TGT=<ビルドの target フォルダ>（既定 target。別ブランチのビルド target-rinrin などを使うとき）、
#   SWITCH_TO=<切り替え先のペット ID>（既定 placeholder。2 体目のペットを確かめるとき。2026-10-27 りんりん）
# 前提: pet-app が起動していないこと
set -u
out=${1:?出力フォルダを指定する}
mode=${2:-single}
to=${SWITCH_TO:-placeholder}
switch=$to; [ "$mode" = rapid ] && switch=$to,mame-dragon
repo=$(cd "$(dirname "$0")/../.." && pwd)
tgt=${TGT:-target}
app="$repo/$tgt/debug/pet-app.exe"
winctl="$repo/$tgt/debug/examples/winctl.exe"
data="$out/data"
rm -rf "$data"; mkdir -p "$data"

PET_CLAUDE_DATA_DIR=$(cygpath -w "$data") PET_TEST_SWITCH_PET=$switch "$app" --open-settings >/dev/null 2>&1 &
log="$data/logs/pet.log"
for _ in $(seq 1 50); do grep -q "pet window created" "$log" 2>/dev/null && break; sleep 0.2; done
sleep 1
rect=$("$winctl" find | sed -n 's/^rect=(\(-\?[0-9]*\),\(-\?[0-9]*\))-(\(-\?[0-9]*\),\(-\?[0-9]*\)).*/\1 \2 \3 \4/p')
read -r l t r b <<<"$rect"
"$winctl" shot $((l - 100)) $((t - 100)) $((r - l + 200)) $((b - t + 120)) "$(cygpath -w "$out/pet-before.png")" >/dev/null
# 設定画面（主画面の中央あたりに出る）を含む主画面全体
"$winctl" shot 0 0 2880 1800 "$(cygpath -w "$out/settings.png")" >/dev/null
mem() { powershell.exe -NoProfile -Command "\$p=Get-Process pet-app|Select-Object -First 1; \$ids=@(\$p.Id)+@(Get-CimInstance Win32_Process|Where-Object {\$_.ParentProcessId -eq \$p.Id}|ForEach-Object {\$_.ProcessId}); '{0:N1}' -f ((\$ids|ForEach-Object {(Get-Process -Id \$_ -ErrorAction SilentlyContinue).PrivateMemorySize64}|Measure-Object -Sum).Sum/1MB)" | tr -d '\r'; }
before=$(mem)
for _ in $(seq 1 40); do grep -q "($to) loaded" "$log" 2>/dev/null && break; sleep 0.25; done
sleep 2   # rapid のとき: 捨てた読み込みの後にもう一度見直す分
sleep 1
"$winctl" shot $((l - 100)) $((t - 100)) $((r - l + 200)) $((b - t + 120)) "$(cygpath -w "$out/pet-after.png")" >/dev/null
after=$(mem)
taskkill //IM pet-app.exe //F >/dev/null 2>&1
sleep 0.5

grep -q "(mame-dragon) loaded" "$log" && r1=PASS || r1=FAIL
grep -q "($to) loaded" "$log" && r2=PASS || r2=FAIL
if [ "$mode" = rapid ]; then
  # 最後に選んだまめドラゴンのまま: 仮キャラへ切り替えた記録が無く、state.json にも仮キャラが無い
  ! grep -q "pet -> $to" "$log" && r3=PASS || r3=FAIL
  ! grep -q "\"pet\": \"$to\"" "$data/state.json" 2>/dev/null && r4=PASS || r4=FAIL
  echo "$r1 起動はまめドラゴン（$(grep -o 'mame-dragon) loaded in [0-9]* ms' "$log")）"
  echo "$r2 $to の読み込みは走った（捨てられる側）"
  echo "$r3 $to へは切り替わらなかった（最後に選んだまめドラゴンのまま）"
  echo "$r4 state.json に $to が残っていない"
  echo "専有メモリ（子孫込み・設定画面を開いたまま）: 前 ${before}MB / 後 ${after}MB"
  [ "$r1$r2$r3$r4" = PASSPASSPASSPASS ]
  exit $?
fi
grep -q "\"pet\": \"$to\"" "$data/state.json" 2>/dev/null && r3=PASS || r3=FAIL
echo "$r1 起動はまめドラゴン（$(grep -o 'mame-dragon) loaded in [0-9]* ms' "$log")）"
echo "$r2 $to へ切り替わった（$(grep -o "$to) loaded in [0-9]* ms" "$log")・$(grep -o 'clips [0-9]*' "$log" | tail -1)）"
echo "$r3 state.json に $to が残った"
echo "専有メモリ（子孫込み・設定画面を開いたまま）: 切り替え前 ${before}MB / 後 ${after}MB"
[ "$r1$r2$r3" = PASSPASSPASS ]
