"""学習用の音の読み込みと水増し（numpy だけ）。

水増しは openWakeWord の学習と同じ考え方: 速さ・音量・残響・雑音を振り、ウェイクワードの終わりを
2 秒の窓の終わり近くに置く（検出は語の終わりで判定するので、学習もそこに合わせる）。
"""

import wave

import numpy as np

import common

RATE = common.RATE


def load_wav(path):
    """WAV → int16 の 1 次元（16kHz・モノラルに直す）"""
    with wave.open(str(path), "rb") as w:
        ch, width, rate, n = w.getnchannels(), w.getsampwidth(), w.getframerate(), w.getnframes()
        raw = w.readframes(n)
    if width != 2:
        raise ValueError(f"{path}: 16bit 以外は読まない（{width * 8}bit）")
    x = np.frombuffer(raw, "<i2").astype(np.float32)
    if ch > 1:
        x = x.reshape(-1, ch).mean(axis=1)
    if rate != RATE:
        x = resample(x, RATE / rate)
    return np.clip(x, -32768, 32767).astype(np.int16)


def resample(x, factor):
    """線形補間で長さを factor 倍にする（速さを 1/factor にするのと同じ。高さも変わる）"""
    x = np.asarray(x, np.float32)
    n = max(1, int(round(len(x) * factor)))
    return np.interp(np.linspace(0, len(x) - 1, n), np.arange(len(x)), x).astype(np.float32)


def trim(x, level=500, pad=800):
    """前後の無音を落とす（最大振幅の割合ではなく絶対値で。合成の音は前後に無音が付く）"""
    x = np.asarray(x)
    loud = np.flatnonzero(np.abs(x.astype(np.int32)) > level)
    if len(loud) == 0:
        return x[:0]
    return x[max(0, loud[0] - pad) : loud[-1] + pad]


def rms(x):
    x = np.asarray(x, np.float32)
    return float(np.sqrt(np.mean(x * x))) if len(x) else 0.0


def noise(n, rng, color=None):
    """白・ピンク・ブラウンの雑音（RMS 1）"""
    color = color or rng.choice(["white", "pink", "brown"])
    w = rng.standard_normal(n).astype(np.float32)
    if color != "white":
        spec = np.fft.rfft(w)
        f = np.arange(len(spec), dtype=np.float32)
        f[0] = 1.0
        spec /= np.sqrt(f) if color == "pink" else f
        w = np.fft.irfft(spec, n).astype(np.float32)
    return w / (rms(w) + 1e-9)


def reverb(x, rng):
    """作った残響（指数で減る雑音を畳み込む）。RT60 0.1〜0.5 秒、混ぜる割合 0〜0.5"""
    rt60 = rng.uniform(0.1, 0.5)
    n = int(RATE * rt60)
    ir = rng.standard_normal(n).astype(np.float32) * np.exp(-6.9 * np.arange(n) / n).astype(np.float32)
    ir /= np.abs(ir).sum() + 1e-9
    wet = np.convolve(x, ir)[: len(x)]
    mix = rng.uniform(0.0, 0.5)
    return (1 - mix) * x + mix * wet * (rms(x) / (rms(wet) + 1e-9))


def augment(x, rng):
    """1 本の音を水増しする（速さ 0.9〜1.1・残響は半分の確率・音量 -12〜+6dB）"""
    y = resample(x, rng.uniform(0.9, 1.1))
    if rng.random() < 0.5:
        y = reverb(y, rng)
    return y * (10 ** (rng.uniform(-12, 6) / 20))


def place_at_end(word, length, rng, background=None, max_gap=0.2, snr=(5, 30)):
    """語を長さ length の窓の終わり近く（終わりから 0〜max_gap 秒）に置き、背景を混ぜる → int16"""
    word = np.asarray(word, np.float32)[-length:]
    gap = int(rng.uniform(0, max_gap) * RATE)
    out = np.zeros(length, np.float32)
    end = length - gap
    start = max(0, end - len(word))
    out[start:end] = word[len(word) - (end - start) :]
    return mix_background(out, rng, background, snr, ref=word)


def mix_background(x, rng, background=None, snr=(5, 30), ref=None):
    """雑音か背景の音（会話・部屋）を SNR snr dB で混ぜる → int16"""
    x = np.asarray(x, np.float32)
    level = rms(ref if ref is not None else x)
    if background is not None and len(background) > len(x) and rng.random() < 0.5:
        s = int(rng.integers(0, len(background) - len(x)))
        bg = np.asarray(background[s : s + len(x)], np.float32)
        bg = bg / (rms(bg) + 1e-9)
    else:
        bg = noise(len(x), rng)
    db = rng.uniform(*snr)
    y = x + bg * (level / (10 ** (db / 20)) if level > 0 else rng.uniform(30, 300))
    return np.clip(y, -32768, 32767).astype(np.int16)
