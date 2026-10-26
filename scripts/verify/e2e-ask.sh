#!/usr/bin/env bash
# 質問（AskUserQuestion）の端から端まで（.steering/20261027-ask-question/brief.md A1・A3・A4・A5・A7）。K の入力を使わない。
#
# 作業用フォルダの試験プロジェクト（自分の git root。親の .claude/settings.json も読まれるが、hook は足すだけ）に
# `--install-hooks --settings` で hooks を登録し、対話の `claude` を ConPTY（claude-tui.py）で動かして AskUserQuestion を
# 呼ばせる。`-p` ではモデルが AskUserQuestion を呼ばない（2026-10-27 司令塔の実測）ので対話モードを使う。
# アプリはデバッグビルドの試験用 PET_AUTO_ASK で、カードのボタンと同じ関数を 1.5 秒後に呼ぶ。
#
#   pick:last → ANSWER=Blue（質問の画面は出ない） / free:むらさき → ANSWER=むらさき / deny → Claude に拒否が届く
#   ask → hook は {} で、いつもの質問の画面が出る / off（設定オフ） → 同じく質問の画面。pet.log に enabled=false
#
# 使い方: bash scripts/verify/e2e-ask.sh <作業用の空フォルダ> [target-dir（既定 target-ask）] [モード（空白区切り。既定は 5 つ全部）]
# 前提: cargo build -p pet-app -p pet-hook 済み・tmp/pty-venv に pywinpty・試験用のアプリが起動していないこと
# （K の常駐のペットは別の名前のミューテックスなので止めなくてよい）
set -u
work=${1:?作業用フォルダを指定する}
# 絶対パスにする。相対のままだと hook（cwd は試験プロジェクト）が PET_CLAUDE_DATA_DIR を別の場所に解決して
# アプリを見つけられない（2026-10-27 に踏んだ: 事象は「hook は走るのに出来事が 1 つも届かない」）
mkdir -p "$work"
work=$(cd "$work" && pwd)
tdir=${2:-target-ask}
repo=$(cd "$(dirname "$0")/../.." && pwd)
app="$repo/$tdir/debug/pet-app.exe"
hook="$repo/$tdir/debug/pet-hook.exe"
py="$repo/tmp/pty-venv/Scripts/python.exe"
[ -x "$app" ] && [ -x "$hook" ] || { echo "先に CARGO_TARGET_DIR=$tdir cargo build -p pet-app -p pet-hook"; exit 2; }
[ -x "$py" ] || { echo "tmp/pty-venv が無い（py -3 -m venv tmp/pty-venv; tmp/pty-venv/Scripts/pip install pywinpty）"; exit 2; }

prompt="Use the AskUserQuestion tool exactly once to ask which color I like, offering the options Red, Green and Blue (header: Color). After I answer, reply with exactly one line: ANSWER=<exactly what I answered>. If the question is declined, rejected or blocked, reply with exactly one line: ANSWER=DECLINED"

modes=${3:-"pick:last free:むらさき deny ask off"}
fail=0
for mode in $modes; do
  name=${mode%%:*}
  proj="$work/$name/proj"
  data="$work/$name/data"
  rm -rf "$work/$name"
  mkdir -p "$proj/.claude" "$data"
  (cd "$proj" && git init -q)
  "$app" --install-hooks --settings "$(cygpath -w "$proj/.claude/settings.json")" >/dev/null
  # 読み上げは止める（試験のたびに AivisSpeech を起こさない）。off は設定「質問にペットで答える」をオフ
  ask_on=true; [ "$mode" = off ] && ask_on=false
  printf '{"version":1,"tts":false,"ask":%s}\n' "$ask_on" > "$data/state.json"
  auto=$mode; [ "$mode" = off ] && auto=ask

  PET_TRACE=1 PET_AUTO_ASK=$auto PET_CLAUDE_DATA_DIR=$(cygpath -w "$data") "$app" >/dev/null 2>&1 &
  app_pid=$!
  for _ in $(seq 1 50); do [ -f "$data/runtime/ipc.json" ] && break; sleep 0.2; done
  if [ ! -f "$data/runtime/ipc.json" ]; then
    echo "[$mode] アプリの受け口が開かない"; kill "$app_pid" 2>/dev/null; fail=1; continue
  fi

  # 期待する画面: 答えが通れば ANSWER=…（プロンプトの echo の ANSWER=< は除く）。任せる・オフなら質問の画面
  # （Claude Code の選択の UI の案内「Enter to select · ↑/↓ to navigate · Esc to cancel」。2026-10-27 実測）
  # Claude の答えの行は「● 」で始まる（プロンプトの echo にも ANSWER= が入るので、印で区別する。2026-10-27 に踏んだ）
  case $mode in
    ask|off) expect='Enter to select' ;;
    *) expect='● ANSWER=' ;;
  esac
  start=$(date +%s)
  "$py" "$repo/scripts/verify/claude-tui.py" --cwd "$proj" --out "$work/$name/screen.txt" --expect "$expect" --timeout 240 \
    --env "PET_CLAUDE_DATA_DIR=$(cygpath -w "$data")" -- --model sonnet "$prompt" >/dev/null
  rc=$?
  took=$(( $(date +%s) - start ))
  sleep 1
  # 試験用のビルド（$tdir の下）の pet-app だけ止める。K の常駐（別のフォルダ）には触らない。
  # bash の $! は exe そのものの pid だが、念のため実行ファイルの場所で選ぶ（wmic は Windows 11 に無い）
  powershell -NoProfile -Command "Get-Process pet-app -ErrorAction SilentlyContinue | Where-Object { \$_.Path -like '*\\$tdir\\*' } | Stop-Process -Force" >/dev/null 2>&1
  wait "$app_pid" 2>/dev/null

  screen="$work/$name/screen.txt"
  log="$data/logs/pet.log"
  answered=$(grep -o "User answered Claude's questions" "$screen" | wc -l)
  answer=$(grep -o "● ANSWER=[^ ]*" "$screen" | sed 's/● //' | tail -1)
  state=$(grep -o "ask [0-9a-f]* state=[a-z]* (accepted=true)" "$log" 2>/dev/null | tail -1 | sed 's/.*state=//; s/ .*//')
  requested=$(grep -c "ask requested" "$log" 2>/dev/null)
  verdict=PASS
  case $mode in
    "pick:last") [ "$answer" = "ANSWER=Blue" ] && [ "$answered" -ge 1 ] && [ "$state" = allow ] || verdict=FAIL ;;
    "free:むらさき") [ "$answer" = "ANSWER=むらさき" ] && [ "$answered" -ge 1 ] && [ "$state" = allow ] || verdict=FAIL ;;
    deny) [ "$answer" = "ANSWER=DECLINED" ] && [ "$state" = deny ] || verdict=FAIL ;;
    ask) [ "$rc" = 0 ] && [ "$state" = ask ] && [ "$answered" = 0 ] || verdict=FAIL ;;
    off) [ "$rc" = 0 ] && grep -q "enabled=false" "$log" && [ "$answered" = 0 ] || verdict=FAIL ;;
  esac
  [ "$verdict" = FAIL ] && fail=1
  echo "[$mode] $verdict tui_rc=$rc answer=${answer:-なし} answered=$answered 要求=${requested:-0} state=${state:-なし} 所要=${took}s"
done
exit $fail
