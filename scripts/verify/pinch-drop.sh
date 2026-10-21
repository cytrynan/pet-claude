#!/usr/bin/env bash
# 本物のマウスで、なでる（クリック）・つまむ（ドラッグ）・置く（離す）を流し、ポーズとクリップの切り替わりを
# ログと撮影で確かめる（.steering/20261021-clip-playback の「なでられ・つまむ・置く」・issue #5）。
# 試験用のデータフォルダで、まめドラゴン・散歩なしの pet-app を PET_TRACE で動かす。K の state.json は書き換えない。
# 本物のカーソルを約 10 秒動かし、最後に元の位置へ戻す。**K の了承を得て流す**。
#
# 使い方: bash scripts/verify/pinch-drop.sh <出力フォルダ>
# ⚠️ 動いている pet-app（K の常駐）を止める（二重起動防止で試験用が立たないため）。最後に元の exe を起動し直す。
set -uo pipefail
cd "$(dirname "$0")/../.."
OUT=${1:?出力フォルダを指定する}
APP="$PWD/target/debug/pet-app.exe"
W="$PWD/target/debug/examples/winctl.exe"
[ -x "$APP" ] && [ -x "$W" ] || { echo "先に cargo build -p pet-app --all-targets"; exit 2; }
mkdir -p "$OUT/shots"
DATA="$OUT/data"
rm -rf "$DATA"; mkdir -p "$DATA"

ORIG=$(powershell -NoProfile -Command "(Get-Process pet-app -ErrorAction SilentlyContinue | Select-Object -First 1).Path")
powershell -NoProfile -Command "Stop-Process -Name pet-app -ErrorAction SilentlyContinue" >/dev/null 2>&1
# 止まりきる前に窓を探すと、閉じかけの常駐の窓を掴む（2026-10-21 実測: 試験用と違う大きさの rect が返った）
for _ in $(seq 1 20); do powershell -NoProfile -Command "if (Get-Process pet-app -ErrorAction SilentlyContinue) { exit 1 }" && break; sleep 0.5; done

# 試験用の state.json: K の位置と大きさをそのまま使い、ペットはまめドラゴン・散歩なし（clips-sweep.sh と同じ）
py -3 -I - "$LOCALAPPDATA/pet-claude/state.json" "$DATA/state.json" <<'EOF'
import json, sys
try:
    s = json.load(open(sys.argv[1], encoding="utf-8"))
except OSError:
    s = {"version": 1}
s.update({"pet": "mame-dragon", "walk": False, "tts": False, "wake": False})
open(sys.argv[2], "w", encoding="utf-8", newline="\n").write(json.dumps(s, ensure_ascii=False, indent=2))
EOF

T0=$(date +%s.%N)
t() { awk -v a="$T0" -v b="$(date +%s.%N)" 'BEGIN{printf "%.1f", b-a}'; }
step() { echo "$(t) $*" | tee -a "$OUT/steps.txt"; }
rect() { "$W" find | sed -nE 's/rect=\(([-0-9]+),([-0-9]+)\)-\(([-0-9]+),([-0-9]+)\).*/\1 \2 \3 \4/p'; }
shoot() {
  local r l t rr b; r=$(rect); [ -n "$r" ] || return   # L T R B は開始時の位置なので上書きしない
  read -r l t rr b <<< "$r"
  "$W" shot "$l" "$t" $((rr-l)) $((b-t)) "$(cygpath -w "$OUT/shots/$(printf '%05.1f' "$(t)")-$1.png")" >/dev/null 2>&1
}
shots() { for _ in $(seq 1 "$2"); do shoot "$1"; sleep 0.3; done; }

powershell -NoProfile -Command "\$env:PET_TRACE='1'; \$env:PET_CLAUDE_DATA_DIR='$(cygpath -w "$DATA")'; Start-Process -FilePath '$(cygpath -w "$APP")' -WindowStyle Hidden"
for _ in $(seq 1 20); do rect | grep -q . && break; sleep 0.5; done
sleep 2   # 待機のクリップを読み終えるまで
read -r _ CX0 CY0 <<<"$("$W" cursor)"
read -r L T R B <<< "$(rect)"
step "start pet ($L,$T)-($R,$B) cursor was $CX0 $CY0"
PX=$(( (L + R) / 2 )); PY=$(( T + (B - T) * 6 / 10 ))   # 胴体のあたり（不透明な画素）

# 1. なでる: クリック 1 回 → Petted（なでられのクリップ）→ 待機へ戻る。クリックはバーも出す
"$W" click "$PX" "$PY" >/dev/null
step "click $PX $PY"
shots petted 8
# 2. つまむ → 動かす → 置く: 300 画素ドラッグ（winctl drag は 30 歩・約 0.6 秒）。
#    作業領域の端で止まらないよう、ペットが画面の左寄りなら右へ、そうでなければ左へ動かす
DX=-300; [ "$L" -lt 600 ] && DX=300
"$W" drag "$PX" "$PY" $((PX + DX)) "$PY" 30 >/dev/null &
DRAG=$!
shots grabbed 3
wait "$DRAG"
step "drag $PX $PY -> $((PX + DX)) $PY"
shots put_down 8
"$W" hover "$CX0" "$CY0" >/dev/null   # カーソルを元へ戻す
step "cursor restored"

cp "$DATA/logs/pet.log" "$OUT/pet.log" 2>/dev/null
powershell -NoProfile -Command "Stop-Process -Name pet-app -ErrorAction SilentlyContinue" >/dev/null 2>&1
sleep 1
if [ -n "$ORIG" ]; then
  powershell -NoProfile -Command "Start-Process -FilePath '$ORIG' -WindowStyle Hidden"
  step "restored $ORIG"
fi

# 判定はログの並びで見る（撮影は目視の裏付け）
LOG="$OUT/pet.log"
ok() { if grep -qE "$2" "$LOG"; then echo "PASS $1"; else echo "FAIL $1（ログに /$2/ が無い）"; FAIL=1; fi; }
FAIL=0
ok "クリックが届いた" "click at \($PX,$PY\)"
ok "なでられのクリップに切り替わった" "show -> petted"
ok "ドラッグが始まった" "drag start"
ok "つまみ中のポーズになった" "pose -> Grabbed"
ok "つまみ中のクリップに切り替わった" "show -> grabbed"
ok "離して置いた" "drag end on"
ok "置かれるのクリップに切り替わった" "show -> put_down"
# 置かれるの後は待機へ戻る
if awk '/show -> put_down/{p=1; next} p && /show -> idle/{f=1} END{exit !f}' "$LOG"; then
  echo "PASS 置かれるの後に待機へ戻った"
else
  echo "FAIL 置かれるの後に待機へ戻っていない"; FAIL=1
fi
# 動いたかは pet-app 自身のログで見る。winctl find は同じクラスの別の窓（並行して走る試験のペット）を
# 掴むことがあり、終わりの rect は当てにならない（2026-10-21 実測: 576x624 のはずが 211x229 が混ざった）
L2=$(sed -nE 's/.*drag end on .* at \((-?[0-9]+),.*/\1/p' "$LOG" | tail -1)
if [ -n "$L2" ] && [ $(( (L2 - L) * DX )) -gt 0 ]; then echo "PASS 窓がドラッグの向きへ動いた（$L → $L2）"
else echo "FAIL 窓が動いていない（$L → ${L2:-不明}）"; FAIL=1; fi
exit "$FAIL"
