"""チャットの故障注入（Phase 3 の A1・A3・A5・A6・A7。`.steering/20261021-phase3-chat/requirements.md`）。

偽の claude（`cargo build -p pet-app --example fake_claude`）を `PET_CLAUDE_EXE` で差し替え、
`PET_TEST_CHAT` でチャットの送信・新しいチャットを本物の関数で行い、ログと子プロセスの残りを見る。
普段のペットは止めなくてよい（試験は別のミューテックス名で起動する。main.rs）。画面にはもう 1 匹ペットが出る。

使い方: py -3 -I scripts/verify/chat-fault.py <pet-app.exe> <fake_claude.exe> <作業用の空フォルダ> [ケース名,...]
（ケース名を付けるとそれだけ回す。名前は run_case の第 1 引数）
"""

import json
import os
import subprocess
import sys
import time
from pathlib import Path

app_exe, fake_exe, work = sys.argv[1], sys.argv[2], Path(sys.argv[3])
only = set(sys.argv[4].split(",")) if len(sys.argv) > 4 else None
SESSION = "11111111-2222-3333-4444-555555555555"
results = []
skipping = False
alive_before_kill = []  # 直前の run_case で、アプリを止める前に生きていた偽の claude の pid  # 直前の run_case がケース名の指定で飛ばされたか（飛ばしたケースの check は数えない）


def alive(pid):
    out = subprocess.run(
        ["tasklist", "/FI", f"PID eq {pid}", "/NH", "/FO", "CSV"],
        capture_output=True, text=True,
    ).stdout
    return f'"{pid}"' in out


def run_case(name, steps, *, mode="ok", exe=None, timeout_secs=None, state=None,
             wait_for=None, max_secs=30, kill_app_after=None, parent_extra=None):
    """pet-app を 1 回起動し、`wait_for` の文がログに出るまで待って止める。(ログ, 作業フォルダ, 生きているか) を返す。"""
    data = work / name
    data.mkdir(parents=True, exist_ok=True)
    global skipping
    skipping = only is not None and name not in only
    if skipping:
        return "", data / "skipped", True
    if state is not None:
        (data / "state.json").write_text(json.dumps(state), encoding="utf-8")
    env = dict(os.environ)
    env.pop("PET_AUTO_DECIDE", None)
    env.update(PET_CLAUDE_DATA_DIR=str(data), PET_CLAUDE_EXE=exe or fake_exe,
               FAKE_MODE=mode, PET_TEST_CHAT=steps)
    # 親に API キーがあってもサブスクでは子に渡らないこと（A1）を見るため、偽の値を入れておく
    env["ANTHROPIC_API_KEY"] = "sk-ant-parent-should-not-leak"
    env["CLAUDECODE"] = "1"
    # 答えの読み上げで本物の AivisSpeech（約 3.4GB）を起こさない（閉じたポートへ向け、起こす exe も無い扱い）
    env.update(PET_TEST_AIVIS_ADDR="127.0.0.1:9", PET_TEST_AIVIS_EXE="")
    if timeout_secs:
        env["PET_CHAT_TIMEOUT_SECS"] = str(timeout_secs)
    env.update(parent_extra or {})
    app = subprocess.Popen([app_exe], env=env)
    log_path = data / "logs" / "pet.log"
    started = time.monotonic()
    log = ""
    while time.monotonic() - started < max_secs:
        log = log_path.read_text(encoding="utf-8") if log_path.exists() else ""
        # 強制終了は、偽の claude とその孫が起動して pid を書いてから（書く前に落とすと何も確かめられない）
        if kill_app_after and kill_app_after in log and len(fake_pids(data / "chat" / "work")) >= 2:
            subprocess.run(["taskkill", "/F", "/PID", str(app.pid)], capture_output=True)
            app.wait(timeout=10)
            time.sleep(1.0)  # OS が Job のハンドルを閉じて中身を止めるのを待つ
            break
        if wait_for and log.count(wait_for[0]) >= wait_for[1]:
            time.sleep(0.5)
            break
        time.sleep(0.2)
    app_alive = app.poll() is None
    global alive_before_kill
    alive_before_kill = [p for p in fake_pids(data / "chat" / "work") if alive(p)]
    if app_alive:
        app.kill()
        app.wait(timeout=10)
    log = log_path.read_text(encoding="utf-8") if log_path.exists() else ""
    return log, data / "chat" / "work", app_alive


def fake_pids(workdir):
    p = workdir / "fake-pids.txt"
    if not p.exists():
        return []
    return [int(line.split()[0]) for line in p.read_text().splitlines() if line.strip()]


def fake_args(workdir):
    p = workdir / "fake-args.txt"
    return p.read_text(encoding="utf-8").splitlines() if p.exists() else []


def check(name, ok, detail):
    if skipping:
        return
    results.append((name, ok, detail))
    print(f"{'PASS' if ok else 'FAIL'} {name}: {detail}")


# A6-1 claude が見つからない
log, wd, alive_ = run_case("notfound", "send:やあ", exe=str(work / "no-such-claude.exe"),
                            wait_for=("chat failed", 1))
check("A6 見つからない", alive_ and "chat failed" in log and not fake_pids(wd),
      f"app 生存={alive_} failed={'chat failed' in log}")

# A6-2 is_error の JSON（鍵違い・上限と同じ形）
log, wd, alive_ = run_case("errorjson", "send:やあ", mode="error", wait_for=("chat failed", 1))
check("A6 is_error", alive_ and "chat failed" in log and "exit=1 is_error=true" in log,
      f"app 生存={alive_}")

# A6-3 JSON でない出力
log, wd, alive_ = run_case("garbage", "send:やあ", mode="garbage", wait_for=("chat failed", 1))
check("A6 壊れた出力", alive_ and "chat failed" in log and "exit=2" in log, f"app 生存={alive_}")

# A6-4 時間切れ（3 秒に縮める）＋ A7 孫まで止まる
log, wd, alive_ = run_case("timeout", "send:やあ", mode="slow", timeout_secs=3,
                            wait_for=("chat failed", 1), max_secs=20)
time.sleep(1.0)
pids = fake_pids(wd)
left = [p for p in pids if alive(p)]
check("A6 時間切れ", alive_ and "timed out after 3" in log, f"app 生存={alive_}")
check("A7 時間切れで子と孫が残らない", len(pids) >= 2 and not left, f"pids={pids} 残り={left}")

# A7 待ち中に新しいチャット
log, wd, alive_ = run_case("cancel", "send:やあ|wait:1500|new|wait:1500", mode="slow",
                            wait_for=("test chat: never", 1), max_secs=7)
pids = fake_pids(wd)
check("A7 新しいチャットで止まる（アプリを止める前に）", "cancelled generation" in log and len(pids) >= 2
      and not alive_before_kill, f"pids={pids} 止める前に生きていた={alive_before_kill}")

# A7 待ち中に pet-app が落ちる（強制終了）
log, wd, alive_ = run_case("crash", "send:やあ", mode="slow", kill_app_after="chat send")
time.sleep(2.0)
pids = fake_pids(wd)
left = [p for p in pids if alive(p)]
check("A7 アプリが落ちても残らない", len(pids) >= 2 and not left, f"pids={pids} 残り={left}")

# 1 本だけ: 待ち中の 2 通目は断る
log, wd, alive_ = run_case("busy", "send:やあ|wait:500|send:もう一回", mode="slow",
                            wait_for=("test chat: submit", 2), timeout_secs=30)
check("1 本だけ", "test chat: submit -> まだ考え中です" in log and len(
    [p for p in fake_pids(wd) if True]) == 2, f"pids={fake_pids(wd)}")

# 1 本だけ（手順がまとめて走る時）: 送る・送る・新しいチャット・送る を待たずに並べる。
# メインスレッドが詰まると実機でもこう並ぶ（2026-10-21 故障注入で、2 本同時に起き、止めるも空振りした）
log, wd, alive_ = run_case("burst", "send:一|send:二|new|send:三|wait:2500", mode="slow",
                            wait_for=("test chat: never", 1), max_secs=8, timeout_secs=30)
time.sleep(1.5)
pids = fake_pids(wd)
left = [p for p in pids if alive(p)]
check("1 本だけ（まとめて来ても）", log.count("submit -> まだ考え中です") == 1
      and "cancelled generation 1" in log and log.count("submit -> 考え中") == 2
      # 止める前に生きているのは、3 通目の子と孫（最後に起きた 2 つ）だけ
      and len(pids) <= 4 and sorted(alive_before_kill) == sorted(pids[-2:]) and not left,
      f"pids={pids} 止める前に生きていた={alive_before_kill} 残り={left}")

# 標準入力を読まない子に大きな本文（2000 字＝6000 バイト。パイプの既定 4KB を超える）を送っても、
# 時間切れの見張りが止まらない（2026-10-21 検証係の指摘）
log, wd, alive_ = run_case("stdin", "send:" + "あ" * 2000, mode="slow", timeout_secs=3,
                            wait_for=("chat failed", 1), max_secs=20)
check("A6 標準入力が詰まっても時間切れ", alive_ and "timed out after 3" in log
      and not alive_before_kill, f"止める前に生きていた={alive_before_kill}")

# 長すぎる本文は起こさずに断る
log, wd, alive_ = run_case("toolong", "send:" + "あ" * 2001, wait_for=("test chat: submit", 1))
check("長すぎる本文は断る", "submit -> 長すぎます" in log and not fake_pids(wd), f"pids={fake_pids(wd)}")

# 「確かめる」: 止まらない CLI でも時間切れで返り、子が残らない（2026-10-21 検証係の指摘）
log, wd, alive_ = run_case("auth", "auth", mode="slow", wait_for=("test chat: auth", 1), max_secs=30)
check("確かめるの時間切れ", "auth -> 時間切れで確かめられませんでした" in log and len(fake_pids(wd)) >= 2
      and not alive_before_kill, f"pids={fake_pids(wd)} 止める前に生きていた={alive_before_kill}")

# 子が動いている最中にチャットのスレッドが panic しても、子と孫が止まり、予約が外れて次を送れる
# （2026-10-21 検証係 2 巡目の指摘。JobGuard と catch_unwind の故障注入）
log, wd, alive_ = run_case("panic", "send:一|wait:3500|send:二|wait:500", mode="slow",
                            wait_for=("test chat: never", 1), max_secs=8,
                            parent_extra={"PET_TEST_CHAT_PANIC": "1500"})
pids = fake_pids(wd)
first = pids[:2]
check("panic でも子が残らず予約が外れる", "chat: worker panicked" in log
      and log.count("submit -> 考え中…") == 2 and len(pids) >= 2
      and not [p for p in first if p in alive_before_kill],
      f"pids={pids} 止める前に生きていた={alive_before_kill}")

# A1・A3・A5 の引数と環境: 続けて送ると --resume、新しいチャットで外れる。親の API キーは渡らない
log, wd, alive_ = run_case("resume", "send:一通目|wait:1500|send:二通目|wait:1500|new|send:三通目",
                            wait_for=("chat answer in", 3))
lines = fake_args(wd)
argv = [json.loads(line.split(" ANTHROPIC_API_KEY=")[0]) for line in lines]
check("A5 2 通目に --resume", len(argv) == 3 and "--resume" not in argv[0]
      and argv[1][-2:] == ["--resume", SESSION] and "--resume" not in argv[2],
      f"{[a[-2:] for a in argv]}")
check("A1 サブスクで親の鍵・CLAUDECODE が子に届かない",
      len(lines) == 3 and all("ANTHROPIC_API_KEY=false" in x and "CLAUDECODE=false" in x for x in lines),
      lines[0][-80:] if lines else "なし")
# 同じ環境で起こす（2026-10-27 K 指示）: 制限の引数が無く、既定のモデル・エフォート・モードと設定の読み込み元が付く
def after(a, flag):
    return a[a.index(flag) + 1] if flag in a else None
def trio(a):
    return (after(a, "--model"), after(a, "--effort"), after(a, "--permission-mode"))
check("同じ環境の引数（既定 Fable・高・自動）", bool(argv) and trio(argv[0]) == ("fable", "high", "auto")
      and after(argv[0], "--setting-sources") == "user,project,local" and "--append-system-prompt-file" in argv[0]
      and not ({"--safe-mode", "--tools", "--disallowedTools", "--strict-mcp-config", "--max-turns", "--system-prompt-file"} & set(argv[0])),
      " ".join(argv[0][:14]) if argv else "なし")
check("N4 ログに本文・答えが無い", "一通目" not in log and "受け取ったよ" not in log
      and "sk-ant-parent" not in log, "ログを grep")

# 設定の既定（state.json の chat_model / chat_effort / chat_mode）が引数に乗り、チャット欄のメニュー（opt:）が
# 次の送信から効き、「新しいチャット」で既定に戻る
log, wd, alive_ = run_case("options", "send:一|wait:1500|opt:sonnet,,manual|send:二|wait:1500|new|send:三",
                            state={"chat_model": "haiku", "chat_effort": "low", "chat_mode": "plan"},
                            wait_for=("chat answer in", 3))
argv = [json.loads(line.split(" ANTHROPIC_API_KEY=")[0]) for line in fake_args(wd)]
trip = [trio(a) for a in argv]
check("設定の既定が引数に乗る", len(trip) == 3 and trip[0] == ("haiku", "low", "plan"), f"{trip}")
check("メニューの値は次の送信から・新しいチャットで既定へ", len(trip) == 3 and trip[1] == ("sonnet", "low", "manual")
      and trip[2] == ("haiku", "low", "plan") and "test chat: opt -> " in log, f"{trip}")

# 知らない値は既定に倒れる（壊れた state.json で bypassPermissions のような広い権限に倒れない）
log, wd, alive_ = run_case("unknownopt", "send:一", state={"chat_model": "gpt", "chat_effort": "ultra", "chat_mode": "root"},
                            wait_for=("chat answer in", 1))
argv = [json.loads(line.split(" ANTHROPIC_API_KEY=")[0]) for line in fake_args(wd)]
check("知らない値は既定（Fable・高・自動）", len(argv) == 1 and trio(argv[0]) == ("fable", "high", "auto"),
      f"{[trio(a) for a in argv]}")

# 作業フォルダ: 設定のフォルダが在ればそこで起こす（偽の claude は cwd に fake-args.txt を書く）。消えていれば既定へ
mydir = work / "chatdir" / "mydir"
mydir.mkdir(parents=True, exist_ok=True)
log, wd, alive_ = run_case("chatdir", "send:一", state={"chat_dir": str(mydir)}, wait_for=("chat answer in", 1))
check("作業フォルダは設定のフォルダ", bool(fake_args(mydir)) and not fake_args(wd), f"mydir={len(fake_args(mydir))} work={len(fake_args(wd))}")
log, wd, alive_ = run_case("chatdirgone", "send:一", state={"chat_dir": str(work / "chatdir" / "gone")}, wait_for=("chat answer in", 1))
check("消えた作業フォルダは既定へ", bool(fake_args(wd)) and not (work / "chatdir" / "gone").exists(), f"work={len(fake_args(wd))}")

# A2 の片側: API キーを選んだのに鍵が無い → 起こさずに失敗（資格情報マネージャーは K の名前なので触らない）
log, wd, alive_ = run_case("nokey", "send:やあ", state={"chat_auth": "api_key"},
                            wait_for=("chat failed", 1))
check("A2 鍵が無ければ起こさない" , alive_ and "chat failed" in log and not fake_pids(wd)
      and "auth=api_key" in log, f"pids={fake_pids(wd)}")

fails = [r for r in results if not r[1]]
print(f"\n{len(results) - len(fails)}/{len(results)} PASS")
sys.exit(1 if fails else 0)
