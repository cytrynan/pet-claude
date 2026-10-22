"""ウェイクワードの道具で共通に使うもの（置き場所・WAV の読み書き）。numpy は要らない。

置き場所は `%LOCALAPPDATA%\\pet-claude\\wake\\`（pet-app と同じく PET_CLAUDE_DATA_DIR で差し替えられる）。
K の録音・合成した音・学習した頭はここに置き、リポジトリには入れない（K の声から作るので）。
"""

import os
import struct
import wave
from pathlib import Path

RATE = 16000


def data_dir():
    base = os.environ.get("PET_CLAUDE_DATA_DIR")
    if base:
        return Path(base)
    return Path(os.environ.get("LOCALAPPDATA", Path.home())) / "pet-claude"


def wake_dir():
    return data_dir() / "wake"


def pet_data(pet):
    """ペットごとの学習データの置き場所（synth-pos・synth-neg・k）"""
    return wake_dir() / "data" / pet


def write_wav(path, samples, rate=RATE):
    """16bit・モノラルで書く。samples は -32768〜32767 の整数の並び"""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(struct.pack("<%dh" % len(samples), *samples))


def read_wav_info(path):
    """(チャンネル数, サンプル幅, レート, フレーム数)"""
    with wave.open(str(path), "rb") as w:
        return w.getnchannels(), w.getsampwidth(), w.getframerate(), w.getnframes()
