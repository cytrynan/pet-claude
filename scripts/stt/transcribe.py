"""音声入力の文字起こし（pet-app の voice.rs が録音ごとに 1 回起こす）。

使い方: python -I transcribe.py <16kHz・モノラル・16bit の WAV>
標準出力に {"text": "...", "secs": 1.23} を 1 行で書く。失敗したら終了コード 1（理由は標準エラー）。

faster-whisper の small を CPU・int8 で使う（2026-10-22 の実測で、6 秒の発話が読み込み 1.7 秒＋認識 1.6 秒。
large-v3 は 15 秒かかり、聞き間違いは同じだった）。区切りは pet-app 側の Endpointer で済んでいるので
vad_filter は切る（小さい声を VAD が捨てる罠を避ける）。前の文に引きずられる幻聴を避けるため
condition_on_previous_text も切る。モデルは HF のキャッシュから読み、実行中は取りに行かない（HF_HUB_OFFLINE=1）。
"""

import json
import sys
import time

MODEL = "small"


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: transcribe.py <wav>", file=sys.stderr)
        return 2
    started = time.perf_counter()
    try:
        from faster_whisper import WhisperModel

        model = WhisperModel(MODEL, device="cpu", compute_type="int8")
        segments, _ = model.transcribe(
            sys.argv[1],
            language="ja",
            beam_size=1,
            vad_filter=False,
            condition_on_previous_text=False,
        )
        text = "".join(s.text for s in segments).strip()
    except Exception as e:  # 理由は pet-app のログに出さない（終了コードだけ見る）ので型名だけ
        print(f"transcribe failed: {type(e).__name__}", file=sys.stderr)
        return 1
    out = {"text": text, "secs": round(time.perf_counter() - started, 2)}
    # ASCII だけで書く（-I で起こすと PYTHONIOENCODING が効かず、パイプは CP932 になる。2026-10-22 故障注入）
    sys.stdout.write(json.dumps(out) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
