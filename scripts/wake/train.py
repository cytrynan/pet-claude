"""ウェイクワードの頭を学習する（openWakeWord の特徴の上の小さな 2 層。numpy だけで学習する）。

    python -I scripts/wake/train.py --pet mame-dragon [--seed 0]

読むもの（`%LOCALAPPDATA%\\pet-claude\\wake\\` の下。どれも無くてよいが、正が 1 本も無ければ止まる）:
  data/<pet>/synth-pos/  AivisSpeech の「ウェイクワード」（synth.py）
  data/<pet>/synth-neg/  AivisSpeech の近い言葉と会話（synth.py）
  data/<pet>/k/          K の録音（record-wake.ps1。word-*＝正、other-*・room-*＝負と背景）
  validation_set_features.npy  openWakeWord の公開の否定データ（特徴量・約 10.7 時間）
書くもの: <pet>.npz（頭。しきい値は evaluate.py が決めて書き直す）

判定に回す分は学習に使わない（split.py の決まりで分ける）: 合成は話者ごと・文ごと、K の録音は後ろの 3 割、
公開データは後ろ半分。
"""

import argparse
import sys
import time
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import audio  # noqa: E402
import common  # noqa: E402
import oww  # noqa: E402
import split  # noqa: E402


def windows_of(feats, step=2):
    """(n, 96) → (m, 16, 96)。16 個ずつ step ずらす"""
    n = feats.shape[0]
    if n < oww.N_FEATURES:
        return np.zeros((0, oww.N_FEATURES, 96), np.float32)
    idx = range(0, n - oww.N_FEATURES + 1, step)
    return np.stack([feats[i : i + oww.N_FEATURES] for i in idx]).astype(np.float32)


def background_bank(neg_clips, k_room):
    """背景に混ぜる音（学習用の負の会話をつないだもの＋K の部屋の音）"""
    parts = [audio.trim(c) for c in neg_clips] + list(k_room)
    parts = [p for p in parts if len(p)]
    return np.concatenate(parts).astype(np.float32) if parts else None


def build(f, data, rng, pos_aug=8, k_aug=40, neg_aug=2, val_step=8):
    """学習用の (X, y, 重み) を作る"""
    bg = background_bank(data["neg"], data["k_room"])
    X, y, w = [], [], []

    def add(win, label, weight):
        X.append(win)
        y.append(label)
        w.append(weight)

    for clip in data["pos"]:
        word = audio.trim(clip)
        for _ in range(pos_aug):
            add(f.last_window(audio.place_at_end(audio.augment(word, rng), oww.CLIP, rng, bg)), 1, 1.0)
    for clip in data["k_pos"]:
        word = audio.trim(clip)
        for _ in range(k_aug):
            add(f.last_window(audio.place_at_end(audio.augment(word, rng), oww.CLIP, rng, bg)), 1, 1.0)
    n_pos = len(X)
    for clip in data["neg"] + data["k_neg"]:
        for _ in range(neg_aug):
            body = audio.augment(audio.trim(clip), rng)
            length = max(oww.CLIP, len(body) + int(rng.uniform(0.3, 1.0) * audio.RATE))
            x = audio.place_at_end(body, length, rng, bg, max_gap=0.5)
            for win in windows_of(f.clip_features(x)):
                add(win, 0, 1.0)
    for _ in range(300):  # 雑音・無音だけ
        x = audio.mix_background(np.zeros(oww.CLIP, np.float32), rng, bg)
        add(f.last_window(x), 0, 1.0)
    n_synth_neg = len(X) - n_pos
    if data["val"] is not None:
        for win in windows_of(data["val"], step=val_step):
            add(win, 0, 1.0)
    X = np.asarray(X, np.float32).reshape(len(X), -1)
    y = np.asarray(y, np.float32)
    w = np.asarray(w, np.float32)
    # 正と負の重みの合計をそろえる（負が桁違いに多いので）
    w[y == 1] *= (y == 0).sum() / max(1, (y == 1).sum())
    print(f"windows: pos={n_pos} neg(synth/K/noise)={n_synth_neg} neg(public)={len(y) - n_pos - n_synth_neg}", flush=True)
    return X, y, w


def train_mlp(X, y, w, hidden=32, epochs=30, lr=1e-3, batch=512, l2=1e-4, seed=0):
    """1536→hidden→1 の 2 層を Adam で学習する（重み付きの交差エントロピー）"""
    rng = np.random.default_rng(seed)
    d = X.shape[1]
    params = {
        "w1": (rng.standard_normal((d, hidden)) * np.sqrt(2.0 / d)).astype(np.float32),
        "b1": np.zeros(hidden, np.float32),
        "w2": (rng.standard_normal((hidden, 1)) * np.sqrt(1.0 / hidden)).astype(np.float32),
        "b2": np.zeros(1, np.float32),
    }
    m = {k: np.zeros_like(v) for k, v in params.items()}
    v = {k: np.zeros_like(p) for k, p in params.items()}
    t = 0
    for ep in range(epochs):
        order = rng.permutation(len(X))
        total = 0.0
        for s in range(0, len(X), batch):
            idx = order[s : s + batch]
            xb, yb, wb = X[idx], y[idx], w[idx]
            h_pre = xb @ params["w1"] + params["b1"]
            h = np.maximum(h_pre, 0)
            z = (h @ params["w2"] + params["b2"]).reshape(-1)
            p = 1 / (1 + np.exp(-np.clip(z, -30, 30)))
            eps = 1e-7
            total += float(np.sum(wb * -(yb * np.log(p + eps) + (1 - yb) * np.log(1 - p + eps))))
            dz = (wb * (p - yb) / wb.sum())[:, None]
            grads = {"w2": h.T @ dz + l2 * params["w2"], "b2": dz.sum(0)}
            dh = (dz @ params["w2"].T) * (h_pre > 0)
            grads["w1"] = xb.T @ dh + l2 * params["w1"]
            grads["b1"] = dh.sum(0)
            t += 1
            for k in params:
                m[k] = 0.9 * m[k] + 0.1 * grads[k]
                v[k] = 0.999 * v[k] + 0.001 * grads[k] ** 2
                mh = m[k] / (1 - 0.9**t)
                vh = v[k] / (1 - 0.999**t)
                params[k] -= (lr * mh / (np.sqrt(vh) + 1e-8)).astype(np.float32)
        if ep % 5 == 4 or ep == epochs - 1:
            print(f"epoch {ep + 1}/{epochs} loss={total / w.sum():.4f}", flush=True)
    return oww.Head(params["w1"], params["b1"], params["w2"].reshape(-1, 1), params["b2"])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pet", required=True)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--epochs", type=int, default=30)
    a = ap.parse_args()
    t0 = time.time()
    data = split.load(a.pet, "train")
    if not data["pos"] and not data["k_pos"]:
        sys.exit("正の音が無い（synth.py か record-wake.ps1 を先に）")
    print(split.describe(data), flush=True)
    f = oww.Features()
    rng = np.random.default_rng(a.seed)
    X, y, w = build(f, data, rng)
    print(f"features {time.time() - t0:.0f}s", flush=True)
    head = train_mlp(X, y, w, epochs=a.epochs, seed=a.seed)
    out = common.wake_dir() / f"{a.pet}.npz"
    head.save(out)
    print(f"saved {out} ({time.time() - t0:.0f}s)")


if __name__ == "__main__":
    main()
