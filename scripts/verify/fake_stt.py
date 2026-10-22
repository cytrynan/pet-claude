"""偽の文字起こし（voice-fault.py が PET_TEST_STT_SCRIPT で差し替える。本物は scripts/stt/transcribe.py）。

動きは環境変数 FAKE_STT で選ぶ:
- ok            : 決まった文を返す
- hallucination : 「ご視聴ありがとうございました。」を返す（V7）
- empty         : 空の文
- garbage       : JSON でない出力
- fail          : 終了コード 1
- slow          : 孫（同じスクリプトを FAKE_STT=sleep で）を起こしてから 600 秒眠る（時間切れ・道連れ V10）
- sleep         : 600 秒眠るだけ（孫の役）
起こされた時に pid を、WAV のパスを受け取ったらパスと有無を、作業フォルダ（pet-app の voice フォルダ）の
fake-stt.txt に追記する。WAV は本物と同じく引数か、標準入力の 1 行目で受ける。
"""

import json
import os
import subprocess
import sys
import time

mode = os.environ.get("FAKE_STT", "ok")


def note(text):
    with open("fake-stt.txt", "a", encoding="utf-8") as f:
        f.write(f"{os.getpid()} {mode} {text}\n")


note("start")
if mode == "sleep":
    time.sleep(600)
    sys.exit(0)
wav = sys.argv[1] if len(sys.argv) > 1 else sys.stdin.readline().strip()
if not wav:
    note("no-path")
    sys.exit(0)
note(f"exists={os.path.exists(wav)} {wav}")

if mode == "slow":
    env = dict(os.environ, FAKE_STT="sleep")
    subprocess.Popen([sys.executable, __file__], env=env)
    time.sleep(600)
elif mode == "fail":
    sys.exit(1)
elif mode == "garbage":
    print("not json")
else:
    text = {"ok": "明日の天気はどうかな", "hallucination": "ご視聴ありがとうございました。", "empty": ""}[mode]
    # 本物と同じく ASCII で（-I で起こされるとパイプは CP932）
    print(json.dumps({"text": text, "secs": 0.1}))
