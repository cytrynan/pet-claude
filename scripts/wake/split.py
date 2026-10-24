"""学習用と判定用の分け方（判定に回す分は学習に使わない）。

- 合成の正: 話者ごと（話者名の crc32 % 7 == 0 を判定へ＝知らない声への強さを測る）
- 合成の負: 文ごと（文の crc32 % 5 == 0 を判定へ）
- K の録音: 番号順の後ろ 3 割を判定へ（word-*・other-* それぞれ）。部屋の音（room-*）は両方の背景に使う
- 公開の否定データ: 前半を学習、後半を判定へ
"""

import json
import zlib

import numpy as np

import audio
import common


def _crc(s):
    return zlib.crc32(s.encode("utf-8"))


def is_eval_speaker(speaker):
    return _crc(speaker) % 7 == 0


def is_eval_text(text):
    return _crc(text) % 5 == 0


def _manifest(d):
    path = d / "manifest.jsonl"
    if not path.exists():
        return []
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            r = json.loads(line)
            if (d / r["file"]).exists():
                rows.append(r)
    return rows


def _tail_split(paths, part):
    paths = sorted(paths)
    n_eval = round(len(paths) * 0.3) if len(paths) >= 3 else 0
    cut = len(paths) - n_eval
    return paths[:cut] if part == "train" else paths[cut:]


def load(pet, part):
    """part は "train" か "eval"。音は int16 の 1 次元の並び"""
    assert part in ("train", "eval")
    want_eval = part == "eval"
    d = common.pet_data(pet)
    pos_rows = [r for r in _manifest(d / "synth-pos") if is_eval_speaker(r["speaker"]) == want_eval]
    neg_rows = [r for r in _manifest(d / "synth-neg") if is_eval_text(r["text"]) == want_eval]
    k = d / "k"
    k_word = _tail_split(k.glob("word-*.wav"), part) if k.exists() else []
    k_other = _tail_split(k.glob("other-*.wav"), part) if k.exists() else []
    k_room = sorted(k.glob("room-*.wav")) if k.exists() else []
    val = None
    vpath = common.wake_dir() / "validation_set_features.npy"
    if vpath.exists():
        v = np.load(vpath, mmap_mode="r")
        half = v.shape[0] // 2
        val = np.asarray(v[:half] if part == "train" else v[half:], np.float32)
    return {
        "pos": [audio.load_wav(d / "synth-pos" / r["file"]) for r in pos_rows],
        "pos_speakers": sorted({r["speaker"] for r in pos_rows}),
        "neg": [audio.load_wav(d / "synth-neg" / r["file"]) for r in neg_rows],
        "neg_texts": [r["text"] for r in neg_rows],
        "k_pos": [audio.load_wav(p) for p in k_word],
        "k_neg": [audio.load_wav(p) for p in k_other],
        "k_room": [audio.load_wav(p) for p in k_room],
        "val": val,
    }


def describe(data):
    val = "なし" if data["val"] is None else f"{data['val'].shape[0] * 0.08 / 3600:.1f} 時間"
    return (
        f"合成の正 {len(data['pos'])} 本（話者 {len(data['pos_speakers'])}）・合成の負 {len(data['neg'])} 本・"
        f"K の正 {len(data['k_pos'])} 本・K の負 {len(data['k_neg'])} 本・部屋 {len(data['k_room'])} 本・公開の負 {val}"
    )
