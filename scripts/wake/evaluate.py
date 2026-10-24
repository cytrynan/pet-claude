"""学習した頭を、学習に使っていない分で測り、しきい値を決めて頭に書き戻す。

    python -I scripts/wake/evaluate.py --pet mame-dragon [--max-fa 0.5] [--no-write]

測るもの（受け入れ条件 W1・W2）:
  - 反応率: 判定用の合成の正（知らない話者）と K の録音。前後に背景を付けた 1 本ずつを流して、ゲート（detect.Gate）が反応したか
  - 誤反応: 公開の否定データの後半（約 5 時間）と、判定用の合成の負をつないだ日本語の会話。ゲートの反応回数を 1 時間あたりに直す
しきい値: 公開データの誤反応が --max-fa 回/時 以下、かつ日本語の会話で 10 分に 1 回以下になる中で、反応率が最も高い（低い方の）値。
結果は `%LOCALAPPDATA%\\pet-claude\\wake\\<pet>-eval.json` にも書く。
"""

import argparse
import json
import sys
import time
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import audio  # noqa: E402
import common  # noqa: E402
import detect  # noqa: E402
import oww  # noqa: E402
import split  # noqa: E402

THRESHOLDS = [round(t, 2) for t in np.arange(0.05, 0.96, 0.05)]


def frame_scores(f, head, x):
    """音 → 80ms ごとの点数（オフラインで全部の embedding を作り、16 個ずつ 1 つずらす）"""
    feats = f.clip_features(x)
    if feats.shape[0] < oww.N_FEATURES:
        return np.zeros(0)
    wins = np.stack([feats[i : i + oww.N_FEATURES] for i in range(feats.shape[0] - oww.N_FEATURES + 1)])
    return head.scores(wins)


def positive_trial(word, rng, bg):
    """語の前に 1.5 秒・後に 1 秒の背景を付けた 1 本（SNR 10〜25dB）"""
    word = audio.trim(word).astype(np.float32)
    pre, post = int(1.5 * audio.RATE), int(1.0 * audio.RATE)
    x = np.zeros(pre + len(word) + post, np.float32)
    x[pre : pre + len(word)] = word
    return audio.mix_background(x, rng, bg, snr=(10, 25), ref=word)


def conversation(clips, rng, bg):
    """判定用の負をつないだ会話（間に 0.2〜1 秒の背景）"""
    parts = []
    for i in rng.permutation(len(clips)):
        parts.append(audio.trim(clips[i]).astype(np.float32))
        parts.append(np.zeros(int(rng.uniform(0.2, 1.0) * audio.RATE), np.float32))
    x = np.concatenate(parts) if parts else np.zeros(0, np.float32)
    return audio.mix_background(x, rng, bg, snr=(15, 30), ref=x[np.abs(x) > 0] if len(x) else None)


def hit_rates(score_lists):
    return {t: (sum(detect.count_events(s, t) > 0 for s in score_lists) / len(score_lists)) if score_lists else None
            for t in THRESHOLDS}


def fa_per_hour(score_lists, seconds):
    hours = seconds / 3600
    return {t: (sum(detect.count_events(s, t) for s in score_lists) / hours) if hours > 0 else None
            for t in THRESHOLDS}


def choose_threshold(rows, max_fa, max_ja=6.0):
    """誤反応の上限（公開データ max_fa 回/時・日本語の会話 max_ja 回/時＝10 分に 1 回）を満たす中で、
    反応率が最も高いしきい値。並んだら低い方（K の声の揺れに余裕を残す）。満たす値が無ければ最も高い値。
    反応率は K の録音があればそれ、無ければ合成の正で測る（K の声が W1 の対象なので）"""
    ok = []
    for t in THRESHOLDS:
        pub = rows["public_fa_per_hour"][t]
        ja = rows["ja_fa_per_hour"][t]
        if (pub is None or pub <= max_fa) and (ja is None or ja <= max_ja):
            ok.append(t)
    recall = rows["k_hit"] if rows["k_hit"][THRESHOLDS[0]] is not None else rows["synth_hit"]
    return max(ok, key=lambda t: (recall[t] or 0.0, -t)) if ok else max(THRESHOLDS)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pet", required=True)
    ap.add_argument("--max-fa", type=float, default=0.5, help="公開データでの誤反応の上限（回/時）")
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--no-write", action="store_true")
    a = ap.parse_args()
    t0 = time.time()
    path = common.wake_dir() / f"{a.pet}.npz"
    head = oww.Head.load(path)
    data = split.load(a.pet, "eval")
    print(split.describe(data), flush=True)
    f = oww.Features()
    rng = np.random.default_rng(a.seed)
    bg = np.concatenate([audio.trim(c) for c in data["neg"]] + data["k_room"]).astype(np.float32) if data["neg"] else None

    synth_scores = [frame_scores(f, head, positive_trial(c, rng, bg)) for c in data["pos"]]
    k_scores = [frame_scores(f, head, positive_trial(c, rng, bg)) for c in data["k_pos"]]
    conv = conversation(data["neg"] + data["k_neg"], rng, bg)
    conv_sec = len(conv) / audio.RATE
    conv_scores = [frame_scores(f, head, conv)] if len(conv) else []
    pub_scores, pub_sec = [], 0.0
    if data["val"] is not None:
        v = data["val"]
        for s in range(0, v.shape[0] - oww.N_FEATURES, 20000):
            block = v[s : min(v.shape[0], s + 20000 + oww.N_FEATURES - 1)]
            wins = np.lib.stride_tricks.sliding_window_view(block, (oww.N_FEATURES, 96))[:, 0]
            pub_scores.append(head.scores(wins))
        pub_sec = v.shape[0] * detect.FRAME_SEC

    rows = {
        "synth_hit": hit_rates(synth_scores),
        "k_hit": hit_rates(k_scores),
        "ja_fa_per_hour": fa_per_hour(conv_scores, conv_sec),
        "public_fa_per_hour": fa_per_hour(pub_scores, pub_sec),
    }
    chosen = choose_threshold(rows, a.max_fa)

    print(f"\n判定用: 合成の正 {len(synth_scores)} 本・K の正 {len(k_scores)} 本・"
          f"日本語の会話 {conv_sec / 60:.1f} 分・公開の負 {pub_sec / 3600:.1f} 時間")
    print("しきい値  合成の反応  Kの反応  会話の誤反応/時  公開の誤反応/時")
    fmt = lambda v, pct=False: "   -   " if v is None else (f"{v * 100:5.1f}%" if pct else f"{v:7.2f}")  # noqa: E731
    for t in THRESHOLDS:
        mark = " ←" if t == chosen else ""
        print(f"  {t:.2f}    {fmt(rows['synth_hit'][t], True)}   {fmt(rows['k_hit'][t], True)}   "
              f"{fmt(rows['ja_fa_per_hour'][t])}      {fmt(rows['public_fa_per_hour'][t])}{mark}")
    report = {
        "pet": a.pet, "threshold": chosen, "max_fa": a.max_fa,
        "counts": {"synth_pos": len(synth_scores), "k_pos": len(k_scores),
                   "ja_minutes": round(conv_sec / 60, 2), "public_hours": round(pub_sec / 3600, 2)},
        "rows": {k: {str(t): v for t, v in r.items()} for k, r in rows.items()},
    }
    (common.wake_dir() / f"{a.pet}-eval.json").write_text(json.dumps(report, ensure_ascii=False, indent=1), encoding="utf-8")
    if not a.no_write:
        head.threshold = chosen
        head.save(path)
    print(f"\nしきい値 {chosen}（{'書き戻した' if not a.no_write else '書き戻していない'}）・{time.time() - t0:.0f}s")


if __name__ == "__main__":
    main()
