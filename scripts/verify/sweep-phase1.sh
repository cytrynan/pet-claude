#!/usr/bin/env bash
# Phase 1 の受入を、いまの HEAD のビルドで一通り測り直す。⚠️ 本物のマウスとキーボードを動かす（数十秒）。
# ⚠️ 持ち主が PC を使っていない時だけ走らせる。2026-10-21 の初回は、試験用の窓を画面外に置いて外側クリックが
#    持ち主の Chrome に当たり、前面が通知だったのに文字を送った（直した: 下の SAFE_* と前面の確認）。
# 使い方: bash scripts/verify/sweep-phase1.sh
# 出力: .steering/20261021-phase1-behavior/evidence/sweep-<commit>/（steps.txt・pet-log.txt・*.png・memory.txt）
set -uo pipefail
cd "$(dirname "$0")/../.."
export PATH="$HOME/.cargo/bin:$PATH"

HEAD=$(git rev-parse --short HEAD)
OUT=.steering/20261021-phase1-behavior/evidence/sweep-$HEAD
mkdir -p "$OUT"
W=./target/debug/examples/winctl.exe
L="$LOCALAPPDATA/pet-claude/logs/pet.log"
STEPS="$OUT/steps.txt"

cargo build -q -p pet-app --all-targets || exit 1
powershell -NoProfile -Command "Stop-Process -Name pet-app -ErrorAction SilentlyContinue" >/dev/null 2>&1
sleep 1
N0=$(wc -l < "$L")
powershell -NoProfile -Command "Remove-Item Env:PET_FAST,Env:PET_TRACE,Env:PET_WV_ARGS -ErrorAction SilentlyContinue; Start-Process -FilePath '$(cygpath -w "$PWD/target/debug/pet-app.exe")' -WindowStyle Hidden"
for _ in $(seq 1 15); do $W find | grep -q rect && break; sleep 1; done
sleep 2

step() { echo "$(date '+%H:%M:%S.%3N') [$HEAD] $*" | tee -a "$STEPS"; }
mem() { powershell -NoProfile -File scripts/verify/measure-memory.ps1 -Attach | tee -a "$OUT/memory.txt" | head -1; }
# ペットの矩形と倍率（物理 px/論理 px）。素材 192×208 × 0.55 × dpi/96
read -r PL PT PR PB < <($W find | sed -E 's/rect=\(([-0-9]+),([-0-9]+)\)-\(([-0-9]+),([-0-9]+)\).*/\1 \2 \3 \4/')
S=$(awk -v w=$((PR-PL)) 'BEGIN{printf "%.4f", w/(192*0.55)}')
px() { awk -v v="$1" -v s="$S" 'BEGIN{printf "%d", v*s}'; }
CX=$(( (PL+PR)/2 )); BY=$(( PT + (PB-PT)*45/100 ))
BAR_Y=$(( PB + $(px 18) ))
step "pet rect=($PL,$PT)-($PR,$PB) scale=$S body=($CX,$BY) bar-center-y=$BAR_Y"
# 外側クリックは試験用のメモ帳の上だけを押す（持ち主の作業中の窓を押さない）。
# メモ帳は、ペットのいる画面の作業領域の中で、ペットから離れた側の上端に置く。置けなければ止める
WORK=$(./target/debug/pet-app.exe --probe 2>/dev/null | tr '|' '\n' \
  | sed -nE 's/.*work=\(([-0-9]+),([-0-9]+)\)-\(([-0-9]+),([-0-9]+)\).*/\1 \2 \3 \4/p' \
  | awk -v x="$CX" -v y="$BY" '$1<=x && x<$3 && $2<=y && y<$4 {print; exit}')
read -r WL WT WR WB <<< "$WORK"
[ -n "${WB:-}" ] || { step "ペットの居る画面の作業領域が分からない。止める"; exit 1; }
NW=$(px 300); NH=$(px 200)
if [ $(( PL - WL )) -gt $(( NW + $(px 260) )) ]; then NX=$(( WL + $(px 40) )); else NX=$(( WR - NW - $(px 40) )); fi
NY=$(( WT + $(px 40) ))
if [ "$NX" -lt "$WL" ] || [ $(( NX + NW )) -gt "$WR" ] || [ $(( NY + NH )) -gt "$WB" ]; then
  step "試験用の窓を作業領域に置けない。止める"; exit 1
fi
$W place Notepad $NX $NY $NW $NH >/dev/null
sleep 0.3
OX=$(( NX + NW/2 )); OY=$(( NY + NH*6/10 ))
# 押す点が本当にメモ帳かを確かめてから使う
$W point $OX $OY | grep -q 'root=Notepad' || { step "($OX,$OY) がメモ帳でない。止める"; exit 1; }
step "試験用のメモ帳を ($NX,$NY) に置いた（作業領域 $WL,$WT-$WR,$WB）。外側クリックは ($OX,$OY)"

step "M0 起動直後のメモリ（UI の窓なし）: $(mem)"

# メニュー（初回作成・バーなし）
$W focus Notepad >/dev/null; sleep 0.4
step "M1 右クリック前の前面: $($W fg)"
$W rclick $CX $BY >/dev/null; sleep 1.5
step "M1 右クリック後の前面: $($W fg)"
ML=$PR; [ $(( PR + $(px 200) )) -gt 2880 ] && [ "$PL" -ge 0 ] && ML=$(( PL - $(px 200) ))
$W shot $(( ML - $(px 20) )) $(( BY - $(px 20) )) $(px 260) $(px 190) "$OUT/m1-menu-first.png" >/dev/null
step "M1 メニューを撮影（初回作成・バーなしで「バーを隠す」が淡色か）"
$W click $OX $OY >/dev/null; sleep 0.8
step "M1 外側（メモ帳）をクリック"

# クリック → 喜ぶ・バー
$W focus Notepad >/dev/null; sleep 0.4
$W click $CX $BY >/dev/null; sleep 0.3
$W shot $(( CX - $(px 120) )) $(( PT - $(px 30) )) $(px 240) $(px 220) "$OUT/b1-jump-0.3s.png" >/dev/null
sleep 1.7
$W shot $(( CX - $(px 120) )) $(( PT - $(px 30) )) $(px 240) $(px 220) "$OUT/b2-bar-2.0s.png" >/dev/null
step "B1 クリック後の前面: $($W fg)"
step "B2 バー表示中のメモリ: $(mem)"

# チャット
$W click $(( CX - $(px 24) )) $BAR_Y >/dev/null; sleep 1.2
step "C1 チャットを押した後の前面: $($W fg)"
$W shot $(( CX - $(px 200) )) $(( PT - $(px 30) )) $(px 400) $(px 220) "$OUT/c1-chat.png" >/dev/null
# 文字を送るのは、前面が pet-claude のチャットのときだけ（違う窓に打たない）
if $W fg | grep -q 'class=Tauri Window title=pet-claude$'; then
  powershell -NoProfile -Command "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('sweep{ENTER}')"
else
  step "C1 前面がチャットでないので文字を送らない"
fi
sleep 0.6
$W shot $(( CX - $(px 200) )) $(( PT - $(px 30) )) $(px 400) $(px 220) "$OUT/c2-chat-submitted.png" >/dev/null
if $W fg | grep -q 'class=Tauri Window title=pet-claude$'; then
  powershell -NoProfile -Command "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('{ESC}')"
fi
sleep 0.8
step "C2 送信→Esc"

# バーを出したままドラッグ
$W click $CX $BY >/dev/null; sleep 1.2
step "D1 バーを出した"
$W down $CX $BY >/dev/null
for d in 10 30 50 70; do $W move $(( CX - $(px "$d") )) $BY >/dev/null; sleep 0.08; done
sleep 0.3
read -r GL GT GR GB < <($W find | sed -E 's/rect=\(([-0-9]+),([-0-9]+)\)-\(([-0-9]+),([-0-9]+)\).*/\1 \2 \3 \4/')
$W shot $(( GL - $(px 20) )) $(( GT - $(px 20) )) $(( GR - GL + $(px 40) )) $(( GB - GT + $(px 100) )) "$OUT/d1-grabbed.png" >/dev/null
$W up >/dev/null; sleep 0.5
step "D1 ドラッグして離した（つままれの撮影・バーが閉じたかはログ）"
$W drag $(( (GL+GR)/2 )) $(( GT + (GB-GT)*45/100 )) $CX $BY 20 >/dev/null; sleep 0.5
step "D2 元の位置へ戻した: $($W find | cut -d' ' -f1)"

# 設定
$W rclick $CX $BY >/dev/null; sleep 1.2
$W click $(( ML + $(px 100) )) $(( BY + $(px 21) )) >/dev/null; sleep 2.5
step "S1 設定を開いた後の前面: $($W fg)"
SC0=$(grep -o '"scale": *[0-9.]*' "$LOCALAPPDATA/pet-claude/state.json")
step "S1 開いた時点の state.json: $SC0（スライダーは触らない＝保存は離したときだけの確認は手動）"

# 片付け → 60 秒後の破棄
$W shot $(( CX - $(px 500) )) $(( PT - $(px 400) )) $(px 1000) $(px 700) "$OUT/s1-around.png" >/dev/null
step "S1 設定の窓を閉じる: $($W closetitle 'pet-claude の設定')"
$W click $OX $OY >/dev/null
step "E0 すべて閉じた。65 秒待つ"
sleep 65
step "E1 65 秒後のメモリ: $(mem)"

tail -n +$((N0 + 1)) "$L" | grep -vE "move cursor" > "$OUT/pet-log.txt"
step "done"
