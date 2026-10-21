#!/usr/bin/env bash
# 動画クリップの再生を、マウスに触らずに一通り撮る（.steering/20261021-clip-playback の R4〜R6）。
# 試験用のデータフォルダで PET_FAST の pet-app を動かし、散歩 → 眠る → 完了の hook（起きる → 喜ぶ）
# → 入力待ちの hook（呼びかけ）→ ツール実行の hook（作業中）を順に起こして、0.5 秒ごとに窓を撮る。
# 最後に待機のまま 20 秒置いて、専有メモリと CPU 時間を測る。
#
# 使い方: bash scripts/verify/clips-sweep.sh <出力フォルダ> [ビルドの target フォルダ（既定 target）] [walk|nowalk] [大きさ] [画面のキー]
# 大きさ（例 1.05）と画面のキー（例 "0,0,2880x1800"。pet-app --probe の先頭）を渡すと、K の state.json の
# その画面での位置に置いて測る（要件 R5 の「105%・200% の画面」用。渡さなければ K の設定のまま）
# nowalk（散歩しない）は待機・眠るの CPU とメモリを測るため（散歩中は毎 tick 窓を動かすので CPU が上がる）
# ⚠️ 動いている pet-app（K の常駐）を止める。終わったら元の exe を同じ引数なしで起動し直す。
set -uo pipefail
cd "$(dirname "$0")/../.."
OUT=${1:?出力フォルダを指定する}
TGT=${2:-target}
WALK=${3:-walk}
SCALE=${4:-}
MON=${5:-}
APP="$PWD/$TGT/debug/pet-app.exe"
HOOK="$PWD/$TGT/debug/pet-hook.exe"
W="$PWD/$TGT/debug/examples/winctl.exe"
[ -x "$APP" ] && [ -x "$HOOK" ] && [ -x "$W" ] || { echo "先に cargo build -p pet-app -p pet-hook --all-targets"; exit 2; }
mkdir -p "$OUT/shots"
DATA="$OUT/data"
rm -rf "$DATA"; mkdir -p "$DATA"

# K の常駐を止める（二重起動防止で試験用が立たないため）。元の exe を覚えておき、最後に戻す
ORIG=$(powershell -NoProfile -Command "(Get-Process pet-app -ErrorAction SilentlyContinue | Select-Object -First 1).Path")
powershell -NoProfile -Command "Stop-Process -Name pet-app -ErrorAction SilentlyContinue" >/dev/null 2>&1
sleep 1

# 試験用の state.json: K の位置と大きさをそのまま使い、ペットはまめドラゴン・散歩あり
py -3 -I - "$LOCALAPPDATA/pet-claude/state.json" "$DATA/state.json" "$WALK" "$SCALE" "$MON" <<'EOF'
import json, sys
src, dst = sys.argv[1], sys.argv[2]
scale, mon = sys.argv[4], sys.argv[5]
try:
    s = json.load(open(src, encoding="utf-8"))
except OSError:
    s = {"version": 1}
s.update({"pet": "mame-dragon", "walk": sys.argv[3] == "walk", "tts": False, "wake": False})
if scale:
    s["scale"] = float(scale)
if mon:
    pos = s.get("positions", {})
    here = [p for p in pos.get("per_monitor", []) if p.get("monitor") == mon]
    if not here:
        sys.exit(f"画面 {mon} での位置が state.json に無い")
    pos["home"] = here[0]
open(dst, "w", encoding="utf-8", newline="\n").write(json.dumps(s, ensure_ascii=False, indent=2))
EOF

T0=$(date +%s.%N)
t() { awk -v a="$T0" -v b="$(date +%s.%N)" 'BEGIN{printf "%.1f", b-a}'; }
step() { echo "$(t) $*" | tee -a "$OUT/steps.txt"; }
hook() { echo "$1" | PET_CLAUDE_DATA_DIR="$(cygpath -w "$DATA")" "$HOOK" >/dev/null 2>&1; step "hook $2"; }
mem() { powershell -NoProfile -File scripts/verify/measure-memory.ps1 -Attach | head -1; }
cpu() { powershell -NoProfile -Command "(Get-Process pet-app).TotalProcessorTime.TotalMilliseconds"; }
shoot() {
  local r; r=$("$W" find | sed -nE 's/rect=\(([-0-9]+),([-0-9]+)\)-\(([-0-9]+),([-0-9]+)\).*/\1 \2 \3 \4/p')
  [ -n "$r" ] || return
  read -r L T R B <<< "$r"
  "$W" shot "$L" "$T" $((R-L)) $((B-T)) "$(cygpath -w "$OUT/shots/$(printf '%05.1f' "$(t)").png")" >/dev/null 2>&1
}
until_t() { while awk -v n="$(t)" -v e="$1" 'BEGIN{exit !(n<e)}'; do shoot; sleep 0.4; done; }

powershell -NoProfile -Command "\$env:PET_FAST='1'; \$env:PET_TRACE='1'; \$env:PET_CLAUDE_DATA_DIR='$(cygpath -w "$DATA")'; Start-Process -FilePath '$(cygpath -w "$APP")' -WindowStyle Hidden"
for _ in $(seq 1 20); do "$W" find | grep -q rect && break; sleep 0.5; done
step "start $("$W" find)"
until_t 26          # 待機 → 散歩（PET_FAST: 5 秒で散歩・20 秒で眠る）→ 眠る
step "mem $(mem)"
S='"session_id":"sweep","cwd":"C:\\\\tmp","transcript_path":"C:\\\\tmp\\\\t.jsonl"'
hook "{\"hook_event_name\":\"Stop\",$S}" "Stop（完了 → 起きる → 喜ぶ）"
until_t 36
hook "{\"hook_event_name\":\"Notification\",\"notification_type\":\"permission_prompt\",\"message\":\"waiting\",$S}" "Notification（入力待ち → 呼びかけ）"
until_t 44
hook "{\"hook_event_name\":\"PreToolUse\",\"tool_name\":\"Read\",\"tool_input\":{\"file_path\":\"C:\\\\\\\\tmp\\\\\\\\a.txt\"},$S}" "PreToolUse（作業中）"
until_t 52
step "mem $(mem)"
hook "{\"hook_event_name\":\"Stop\",$S}" "Stop（作業の終わり → 喜ぶ → 待機）"
until_t 58
# 待機（nowalk なら散歩しない）の CPU。PET_FAST は 20 秒で眠るので、直前の Stop から 10 秒の間に測る
C0=$(cpu); sleep 10; C1=$(cpu)
step "cpu $WALK 10s: $(awk -v a="$C0" -v b="$C1" 'BEGIN{printf "%.2f%% of one core", (b-a)/100}')"
step "mem $(mem)"
cp "$DATA/logs/pet.log" "$OUT/pet.log" 2>/dev/null
# シートの読み込みが 1 回か（解像度の上限を変えたとき、読み直しが止まらなくなったことがある）と、K の入力が混ざっていないか
step "シートの読み込み $(grep -c "pet '" "$OUT/pet.log") 回・K の入力 $(grep -cE 'button up|bar on hover|drag start' "$OUT/pet.log") 回"

powershell -NoProfile -Command "Stop-Process -Name pet-app -ErrorAction SilentlyContinue" >/dev/null 2>&1
sleep 1
if [ -n "$ORIG" ]; then
  powershell -NoProfile -Command "Remove-Item Env:PET_FAST,Env:PET_TRACE,Env:PET_CLAUDE_DATA_DIR -ErrorAction SilentlyContinue; Start-Process -FilePath '$ORIG' -WindowStyle Hidden"
  step "restored $ORIG"
fi
