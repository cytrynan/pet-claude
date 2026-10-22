"""偽の文字起こし（voice-fault.py が PET_TEST_STT_SCRIPT で差し替える。本物は scripts/stt/transcribe.py）。

動きは環境変数 FAKE_STT で選ぶ:
- ok            : 決まった文を返す
- hallucination : 「ご視聴ありがとうございました。」を返す（V7）
- empty         : 空の文
- garbage       : JSON でない出力
- fail          : 終了コード 1
- slow          : 孫（同じスクリプトを FAKE_STT=sleep で）を起こしてから 600 秒眠る（時間切れ・道連れ V10）
- sleep         : 600 秒眠るだけ（孫の役）
起こされた時の pid と、受け取った WAV のパスと有無を、WAV と同じフォルダの fake-stt.txt に追記する。
"""

import json
import os
import subprocess
import sys
import time

mode = os.environ.get("FAKE_STT", "ok")
wav = sys.argv[-1]
with open(os.path.join(os.path.dirname(wav), "fake-stt.txt"), "a", encoding="utf-8") as f:
    f.write(f"{os.getpid()} {mode} exists={os.path.exists(wav)} {wav}\n")

if mode == "sleep":
    time.sleep(600)
elif mode == "slow":
    env = dict(os.environ, FAKE_STT="sleep")
    subprocess.Popen([sys.executable, __file__, wav], env=env)
    time.sleep(600)
elif mode == "fail":
    sys.exit(1)
elif mode == "garbage":
    print("not json")
else:
    text = {"ok": "明日の天気はどうかな", "hallucination": "ご視聴ありがとうございました。", "empty": ""}[mode]
    # 本物と同じく ASCII で（-I で起こされるとパイプは CP932）
    print(json.dumps({"text": text, "secs": 0.1}))
