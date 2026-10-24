"""AivisSpeech でウェイクワードの学習用の音を合成する（正: ウェイクワード、負: 近い言葉と無関係な会話）。

    python -I scripts/wake/synth.py --word まめちゃん --pet mame-dragon [--per-style 3] [--seed 1]

- AivisSpeech Engine（127.0.0.1:10101）が動いている必要がある。動いていなければ止まる（起こさない）
- 出力は `%LOCALAPPDATA%\\pet-claude\\wake\\data\\<pet>\\synth-pos\\` と `synth-neg\\` に 16kHz・モノラルの WAV と
  `manifest.jsonl`（文・話者・スタイル・速さなど）。同じ seed なら同じ並びで、作り済みのファイルは飛ばす（途中から再開できる）
- 声の幅を出すため、入っている全話者・全スタイルを使い、速さ・抑揚・前後の無音を振る
"""

import argparse
import json
import random
import sys
import urllib.parse
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common  # noqa: E402
import texts  # noqa: E402

ENGINE = "http://127.0.0.1:10101"


def get(path, timeout=10):
    with urllib.request.urlopen(ENGINE + path, timeout=timeout) as r:
        return r.read()


def post(path, body=None, timeout=60):
    data = json.dumps(body).encode("utf-8") if body is not None else b""
    req = urllib.request.Request(ENGINE + path, data=data, method="POST", headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def styles():
    """(話者名, スタイル名, id) の並び。「選択しないでください」と書かれたスタイルは使わない"""
    out = []
    for sp in json.loads(get("/speakers")):
        for st in sp["styles"]:
            if "選択しな" in st["name"]:
                continue
            out.append((sp["name"], st["name"], st["id"]))
    return out


def synthesize(text, style_id, params):
    q = json.loads(post("/audio_query?" + urllib.parse.urlencode({"text": text, "speaker": style_id})))
    q.update(params)
    q["outputSamplingRate"] = common.RATE
    q["outputStereo"] = False
    return post("/synthesis?" + urllib.parse.urlencode({"speaker": style_id}), q)


def plan(word, all_styles, per_style, seed):
    """作る音の一覧 [(種類, 名前, 文, スタイル, 速さなど)]。seed が同じなら同じ一覧"""
    rng = random.Random(seed)
    jobs = []

    def params():
        return {
            "speedScale": round(rng.uniform(0.85, 1.25), 3),
            "intonationScale": round(rng.uniform(0.7, 1.5), 3),
            "pitchScale": round(rng.uniform(-0.04, 0.04), 3),
            "prePhonemeLength": round(rng.uniform(0.05, 0.25), 3),
            "postPhonemeLength": round(rng.uniform(0.1, 0.3), 3),
        }

    pos_texts = texts.positives(word)
    for st in all_styles:
        for _ in range(per_style):
            jobs.append(("pos", rng.choice(pos_texts), st, params()))
    for near in texts.near_misses(word):
        for st in rng.sample(all_styles, min(6, len(all_styles))):
            jobs.append(("neg", near, st, params()))
    for sentence in texts.SENTENCES:
        for st in rng.sample(all_styles, min(6, len(all_styles))):
            jobs.append(("neg", sentence, st, params()))
    named, count = [], {"pos": 0, "neg": 0}
    for kind, text, st, p in jobs:
        count[kind] += 1
        named.append((kind, f"{kind}-{count[kind]:05d}", text, st, p))
    return named


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--word", required=True)
    ap.add_argument("--pet", required=True)
    ap.add_argument("--per-style", type=int, default=3)
    ap.add_argument("--seed", type=int, default=1)
    a = ap.parse_args()
    try:
        all_styles = styles()
    except OSError as e:
        sys.exit(f"AivisSpeech Engine に繋がらない（{ENGINE}）: {e}")
    out = common.pet_data(a.pet)
    jobs = plan(a.word, all_styles, a.per_style, a.seed)
    print(f"styles={len(all_styles)} jobs={len(jobs)} out={out}", flush=True)
    made = skipped = failed = 0
    for k in ("pos", "neg"):
        (out / f"synth-{k}").mkdir(parents=True, exist_ok=True)
    manifests = {k: open(out / f"synth-{k}" / "manifest.jsonl", "a", encoding="utf-8") for k in ("pos", "neg")}
    for i, (kind, name, text, (sp, stn, sid), p) in enumerate(jobs):
        path = out / f"synth-{kind}" / f"{name}.wav"
        if path.exists():
            skipped += 1
            continue
        try:
            wav = synthesize(text, sid, p)
        except OSError as e:
            failed += 1
            print(f"失敗 {name} {sp}/{stn}: {e}", flush=True)
            continue
        path.write_bytes(wav)
        manifests[kind].write(json.dumps({"file": path.name, "text": text, "speaker": sp, "style": stn, **p},
                                         ensure_ascii=False) + "\n")
        made += 1
        if made % 50 == 0:
            print(f"{i + 1}/{len(jobs)} made={made}", flush=True)
    for m in manifests.values():
        m.close()
    print(f"done made={made} skipped={skipped} failed={failed}")


if __name__ == "__main__":
    main()
