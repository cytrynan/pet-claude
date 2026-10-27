#!/usr/bin/env bash
# マニュアル用に、実機のペット・バー・右クリックメニュー・承認のカード・質問のカード・固定文言の吹き出しを撮る（2026-10-28）。
# 試しのペットを PET_CLAUDE_DATA_DIR を分けて起こし（常駐のペットは止めない）、自分が起こした PID だけ止める。
# 背景には無地の最前面の窓（backdrop.ps1）を敷き、K のデスクトップの中身を写さない。座標はすべて物理ピクセル。
# 使い方: bash docs/manual/capture-shots.sh <出力フォルダ>
# 前提: cargo build -p pet-app --all-targets && cargo build -p pet-hook（pet-app・pet-hook・examples/winctl）
set -eu
out=${1:?出力フォルダを指定する}
repo=$(cd "$(dirname "$0")/../.." && pwd)
here=$(cd "$(dirname "$0")" && pwd)
app="$repo/target/debug/pet-app.exe"
hook="$repo/target/debug/pet-hook.exe"
W="$repo/target/debug/examples/winctl.exe"
data_dir="$repo/target/manual-data"
data=$(cygpath -w "$data_dir")
raw=$(cygpath -w "$out")
mkdir -p "$out"
rm -rf "$data_dir"; mkdir -p "$data_dir"
# 大きめ（90%）のまめドラゴン、読み上げと散歩はオフ
printf '{"scale":0.9,"pet":"mame-dragon","tts":false,"walk":false}' > "$data_dir/state.json"

PET_CLAUDE_DATA_DIR="$data" "$app" >/dev/null 2>&1 &
for _ in $(seq 1 60); do [ -f "$data_dir/runtime/ipc.json" ] && break; sleep 0.25; done
pid=$(python -c "import json,sys;print(json.load(open(sys.argv[1]))['pid'])" "$data_dir/runtime/ipc.json")
trap 'taskkill //PID "$pid" //F >/dev/null 2>&1 || true' EXIT
sleep 4
rect() { powershell.exe -NoProfile -File "$here/find-pet-window.ps1" "$pid"; }
read -r l t r b <<<"$(rect)"
cx=$(( (l + r) / 2 )); cy=$(( (t + b) / 2 ))
# 既定の置き場所（主画面の右下）だとバーが上に出るので、左上へ動かす
"$W" drag "$cx" "$cy" $((cx - 400)) $((cy - 500)) 20
sleep 2
read -r l t r b <<<"$(rect)"
cx=$(( (l + r) / 2 )); cy=$(( (t + b) / 2 ))
echo "pet rect=($l,$t)-($r,$b)"
powershell.exe -NoProfile -File "$here/backdrop.ps1" -X $((l - 700)) -Y $((t - 800)) -W $((r - l + 1400)) -H $((b - t + 1200)) -Seconds 100 >/dev/null 2>&1 &
sleep 4
away() { "$W" move $((l - 400)) $((t - 200)); }

away; sleep 1.5
"$W" shot $((l - 100)) $((t - 120)) $((r - l + 200)) $((b - t + 330)) "$raw\\idle.png"
# クリックでバー。もう一度クリックして閉じ、カーソルを離す（乗ったままだとホバーでまた出る）
"$W" click "$cx" "$cy"; sleep 1.5
"$W" shot $((l - 300)) $((t - 120)) $((r - l + 600)) $((b - t + 330)) "$raw\\bar.png"
"$W" click "$cx" "$cy"; away; sleep 1.5
# 右クリックでメニュー。右クリックの直後にカーソルを離す（ホバーのバーが混ざらないように）。外をクリックして閉じる
"$W" rclick "$cx" "$cy"; away; sleep 1.5
"$W" shot $((l - 300)) $((t - 120)) $((r - l + 750)) $((b - t + 330)) "$raw\\menu.png"
"$W" click $((l - 250)) $((t - 100)); away; sleep 1
# 承認のカード（CLI から来た形。hook は 14 秒で諦める）
printf '{"hook_event_name":"PermissionRequest","session_id":"manual","tool_name":"Bash","tool_input":{"command":"cargo test -p pet-core"}}' |
  PET_CLAUDE_DATA_DIR="$data" PET_PERMISSION_WAIT_SECS=14 CLAUDE_CODE_ENTRYPOINT=cli "$hook" >/dev/null 2>&1 &
sleep 4
"$W" shot $((l - 500)) $((t - 400)) $((r - l + 800)) $((b - t + 450)) "$raw\\perm.png"
sleep 12
# 質問のカード（選択肢 3 つ・説明つき）
printf '{"hook_event_name":"PreToolUse","session_id":"manual","tool_name":"AskUserQuestion","tool_input":{"questions":[{"question":"どの色にしますか？","header":"色","options":[{"label":"赤","description":"あたたかい"},{"label":"青","description":"落ち着く"},{"label":"緑","description":"やさしい"}],"multiSelect":false}]}}' |
  PET_CLAUDE_DATA_DIR="$data" PET_ASK_WAIT_SECS=14 CLAUDE_CODE_ENTRYPOINT=cli "$hook" --ask >/dev/null 2>&1 &
sleep 4
"$W" shot $((l - 500)) $((t - 700)) $((r - l + 800)) $((b - t + 750)) "$raw\\ask.png"
sleep 12
# 固定文言「できたよ」（3 秒で消えるので出ている間に撮る）
printf '{"hook_event_name":"Stop","session_id":"manual"}' | PET_CLAUDE_DATA_DIR="$data" "$hook" >/dev/null 2>&1
sleep 1.2
"$W" shot $((l - 300)) $((t - 200)) $((r - l + 600)) $((b - t + 250)) "$raw\\status.png"
ls -la "$out"
