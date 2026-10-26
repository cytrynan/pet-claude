"""吹き出しの固定文言を実機で撮る（長い文と短い文）。常駐のペットは止めない。

別のデータフォルダ（`PET_CLAUDE_DATA_DIR`＝別名のミューテックス）で 2 体目を起こし、マイクの代わりに
ミュートのマイクの形の WAV（最大振幅 1）を流し込んで「マイクを使えないよ（ミュートか Windows の設定）」を出す
（2026-10-27 K の報告で切れていた文。voice-fault.py の voice-muted と同じ仕掛け）。3 秒で消えた後、Stop の hook で
「できたよ」（短い文＝従来のカプセル）も撮る。止めるのは自分が起こした PID だけ。

使い方: py -3 -I scripts/verify/shot-status.py <target のフォルダ名> <出力フォルダ>
  例:   py -3 -I scripts/verify/shot-status.py target-bubble C:\\tmp\\shot
前提: cargo build -p pet-app --all-targets（pet-app・pet-hook・examples/winctl）。デバッグビルドだけ（試験用の差し替えが効く）
"""

import json
import os
import re
import struct
import subprocess
import sys
import time
from pathlib import Path

repo = Path(__file__).resolve().parents[2]
target = repo / sys.argv[1]
out = Path(sys.argv[2])
app = target / "debug/pet-app.exe"
hook = target / "debug/pet-hook.exe"
winctl = target / "debug/examples/winctl.exe"
fake_stt = Path(__file__).with_name("fake_stt.py")
for exe in (app, hook, winctl):
    if not exe.exists():
        sys.exit(f"無い: {exe}（先に cargo build -p pet-app --all-targets）")
out.mkdir(parents=True, exist_ok=True)
data = out / "data"
if data.exists():
    import shutil

    shutil.rmtree(data)
data.mkdir()
# 声は鳴らさない（AivisSpeech を起こさない）。大きさは普段の 1.5
(data / "state.json").write_text(json.dumps({"version": 1, "scale": 1.5, "tts": False}), encoding="utf-8")
muted = out / "muted.wav"
samples = [(i % 3) - 1 for i in range(96000)]  # -1〜1 だけの 6 秒
pcm = struct.pack(f"<{len(samples)}h", *samples)
muted.write_bytes(
    b"RIFF" + struct.pack("<I", 36 + len(pcm)) + b"WAVE" + b"fmt "
    + struct.pack("<IHHIIHH", 16, 1, 1, 16000, 32000, 2, 16) + b"data" + struct.pack("<I", len(pcm)) + pcm
)
env = dict(os.environ)
env.pop("PET_AUTO_DECIDE", None)
env.update(
    PET_CLAUDE_DATA_DIR=str(data), PET_TEST_CHAT="ui:voice", PET_TEST_VOICE_WAV=str(muted),
    PET_TEST_STT_EXE=sys.executable, PET_TEST_STT_SCRIPT=str(fake_stt), FAKE_STT="ok",
    PET_TEST_AIVIS_EXE="", PET_TEST_AIVIS_ADDR="127.0.0.1:1",
)
log_path = data / "logs/pet.log"


def log():
    return log_path.read_text(encoding="utf-8", errors="replace") if log_path.exists() else ""


def wait_log(needle, secs):
    t0 = time.monotonic()
    while time.monotonic() - t0 < secs:
        if needle in log():
            return True
        time.sleep(0.2)
    return False


def pet_rect():
    # 起動時の配置の行: relayout(startup) monitors=N -> ... at (x,y) WxH
    m = re.search(r"relayout\(startup\).* at \((-?\d+),(-?\d+)\) (\d+)x(\d+)", log())
    if not m:
        sys.exit("ペットの位置がログに無い")
    x, y, w, h = map(int, m.groups())
    return x, y, w, h


def shot(name):
    x, y, w, h = pet_rect()
    # ペットの上 700px（吹き出しは真上。作業領域の端では横に寄る）とペット本体。左へ 1300px
    left, top = max(x + w - 1300, x - 700), max(y - 700, 0)
    subprocess.run([str(winctl), "shot", str(left), str(top), str(x + w - left + 40), str(y + h - top), str(out / name)], check=True)


proc = subprocess.Popen([str(app)], env=env)
print("test pet pid", proc.pid)
try:
    # 試験用のミューテックスは 1 つ（別のデータフォルダでも「試験」同士は並べない）。他の試験が動いていれば止まる
    for _ in range(75):
        if "relayout(startup)" in log() or "既に起動しているので終了" in log():
            break
        time.sleep(0.2)
    if "既に起動しているので終了" in log():
        sys.exit("別の試験用の pet-app が動いている（試験用のミューテックスは 1 つ）。それが終わってから実行する")
    if "relayout(startup)" not in log():
        sys.exit("pet-app が起動しない（ログ: %s）" % log_path)
    print("long: voice failed", wait_log("voice: failed", 40), "bubble", wait_log("bubble window created", 10))
    time.sleep(0.9)  # 吹き出しの WebView2 が描くまで
    shot("status-long.png")
    time.sleep(3.5)  # 固定文言は 3 秒で消える
    for _ in range(50):
        if (data / "runtime/ipc.json").exists():
            break
        time.sleep(0.2)
    subprocess.run([str(hook)], input=b'{"hook_event_name":"Stop","session_id":"shot"}',
                   env={**os.environ, "PET_CLAUDE_DATA_DIR": str(data)}, timeout=20)
    time.sleep(1.2)
    shot("status-short.png")
finally:
    subprocess.run(["taskkill", "/F", "/T", "/PID", str(proc.pid)], capture_output=True)
    proc.wait(timeout=10)
for line in log().splitlines():
    if "voice:" in line or "bubble" in line:
        print(line.encode("ascii", "replace").decode())
print("saved", out / "status-long.png", out / "status-short.png")
