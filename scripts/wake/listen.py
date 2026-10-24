"""ウェイクワードを聞き続ける子プロセス（pet-app が起こし、マイクの音を標準入力に流す）。

    python -I scripts/wake/listen.py --pet mame-dragon [--threshold 0.5]

- 標準入力: 16kHz・モノラル・16bit（リトルエンディアン）の生の音。閉じられたら終わる
- 標準出力: 1 行 1 つの JSON（ASCII）。読み込みが済んだら {"event":"ready"}、呼ばれたら {"event":"wake","score":0.93}
- 頭（%LOCALAPPDATA%\\pet-claude\\wake\\<pet>.npz）が無い・壊れている時は {"event":"error","reason":…} を書いて終了コード 2
- 音はどこにも保存しない（N4）
"""

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common  # noqa: E402
import detect  # noqa: E402


def say(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pet")
    ap.add_argument("--model")
    ap.add_argument("--threshold", type=float)
    a = ap.parse_args()
    try:
        import numpy as np

        import oww

        path = Path(a.model) if a.model else common.wake_dir() / f"{a.pet}.npz"
        head = oww.Head.load(path)
        stream = oww.Stream(oww.Features())
        gate = detect.Gate(a.threshold or head.threshold)
    except Exception as e:  # noqa: BLE001 — 何が起きても 1 行の理由にして pet-app に渡す
        say({"event": "error", "reason": f"{type(e).__name__}: {e}"[:200]})
        sys.exit(2)
    say({"event": "ready", "threshold": gate.threshold})
    src = sys.stdin.buffer
    while True:
        buf = src.read(oww.CHUNK * 2)
        if not buf:
            break
        x = np.frombuffer(buf[: len(buf) // 2 * 2], "<i2")
        for win in stream.push(x):
            score = float(head.scores(win)[0])
            if gate.push(score):
                say({"event": "wake", "score": round(score, 3)})


if __name__ == "__main__":
    main()
