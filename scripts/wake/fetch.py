"""ウェイクワードに要るファイルを取ってくる（1 回だけ。K の了承のうえで走らせる）。

    python -I scripts/wake/fetch.py [--no-validation]

置き場所: %LOCALAPPDATA%\\pet-claude\\wake\\（リポジトリには入れない）
  models/melspectrogram.onnx    openWakeWord v0.5.1 の特徴抽出（約 1.1MB）
  models/embedding_model.onnx   同上（約 1.3MB）
  validation_set_features.npy   openWakeWord の公開の否定データ（特徴量。約 185MB・約 10.7 時間分）。誤反応率を測るのに使う

ライセンス: openWakeWord のコードは Apache-2.0 だが、README は「同梱の学習済みモデルは CC BY-NC-SA 4.0」、
否定データ（Hugging Face の davidscripka/openwakeword_features）も CC BY-NC-SA 4.0（2026-10-24 確認）。
非営利の個人利用に限る。ここから学習した頭も同じ扱いにする。

大きさと SHA-256 を確かめ、違えば消して止まる。既にあって一致すれば何もしない。
"""

import argparse
import hashlib
import sys
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import common  # noqa: E402

REL = "https://github.com/dscripka/openWakeWord/releases/download/v0.5.1/"
HF = "https://huggingface.co/datasets/davidscripka/openwakeword_features/resolve/main/"

# (置き場所, URL, 大きさ, SHA-256)。大きさは 2026-10-24 に HEAD で確かめた値。
# 否定データの SHA-256 は Hugging Face が公開している LFS の値（X-Linked-ETag）。GitHub のリリースは
# SHA-256 を出していないので、ONNX の 2 つは None（大きさだけ確かめ、取った時に出る値をここに書く）
FILES = [
    ("models/melspectrogram.onnx", REL + "melspectrogram.onnx", 1087958, None),
    ("models/embedding_model.onnx", REL + "embedding_model.onnx", 1326578, None),
    ("validation_set_features.npy", HF + "validation_set_features.npy", 184836608,
     "a56a8a0f8e0efb91900acc6de4c0cdf4c564842e8475a7d49b36c039e17a690f"),
]


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def ok(path, size, digest):
    return path.exists() and path.stat().st_size == size and (digest is None or sha256(path) == digest)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--no-validation", action="store_true")
    a = ap.parse_args()
    root = common.wake_dir()
    for rel, url, size, digest in FILES:
        if a.no_validation and rel.endswith(".npy"):
            continue
        dest = root / rel
        if ok(dest, size, digest):
            print(f"ある: {dest}")
            continue
        dest.parent.mkdir(parents=True, exist_ok=True)
        tmp = dest.with_suffix(dest.suffix + ".part")
        print(f"取る: {url}（{size / 1e6:.1f}MB）", flush=True)
        urllib.request.urlretrieve(url, tmp)
        if tmp.stat().st_size != size or (digest and sha256(tmp) != digest):
            tmp.unlink()
            sys.exit(f"大きさか SHA-256 が違う: {url}")
        tmp.replace(dest)
        print(f"  sha256 {sha256(dest)}")


if __name__ == "__main__":
    main()
