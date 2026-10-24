"""openWakeWord と同じ特徴量（numpy・onnxruntime で動かす。openwakeword のパッケージは入れない）。

    音（16kHz・int16）→ melspectrogram.onnx（約 97 フレーム/秒・32 次元。x/10+2 に直す）
      → embedding_model.onnx（76 フレームの窓を 8 フレームずつずらす＝80ms ごとに 96 次元）
      → 頭（直近 16 個＝1.28 秒）

前処理の定数と流し方（1280 サンプルずつ・前に 480 サンプル足して melspec）は openWakeWord 0.6 の
`openwakeword/utils.py` の AudioFeatures と同じにしている（学習も検出も同じ特徴の上で動くように）。
モデルの 2 ファイルは `fetch.py` が `%LOCALAPPDATA%\\pet-claude\\wake\\models\\` に置く。
"""

import numpy as np

import common

CHUNK = 1280  # 80ms
WINDOW = 76  # embedding の 1 回に使う melspec のフレーム数
STEP = 8  # embedding をずらすフレーム数（＝80ms）
N_FEATURES = 16  # 頭が見る embedding の数（1.28 秒）
CLIP = 32000  # 16 個の embedding を作るのに要る長さ（2 秒）


def _session(path):
    # onnxruntime はここで読む（頭と流し方のテストを onnxruntime の無い CI でも走らせるため）
    import onnxruntime as ort

    o = ort.SessionOptions()
    o.inter_op_num_threads = 1
    o.intra_op_num_threads = 1
    return ort.InferenceSession(str(path), sess_options=o, providers=["CPUExecutionProvider"])


class Features:
    def __init__(self, models_dir=None):
        d = models_dir or (common.wake_dir() / "models")
        self.mel = _session(d / "melspectrogram.onnx")
        self.emb = _session(d / "embedding_model.onnx")
        self.mel_in = self.mel.get_inputs()[0].name
        self.emb_in = self.emb.get_inputs()[0].name

    def melspec(self, x):
        """int16 の 1 次元 → (フレーム, 32)"""
        x = np.asarray(x, dtype=np.int16).astype(np.float32)[None, :]
        spec = np.squeeze(self.mel.run(None, {self.mel_in: x})[0])
        return spec / 10.0 + 2.0

    def embed_windows(self, spec):
        """(フレーム, 32) → (n, 96)。76 フレームの窓を 8 ずつずらす。足りなければ (0, 96)"""
        wins = [spec[i : i + WINDOW] for i in range(0, spec.shape[0] - WINDOW + 1, STEP)]
        if not wins:
            return np.zeros((0, 96), np.float32)
        batch = np.asarray(wins, np.float32)[:, :, :, None]
        return self.emb.run(None, {self.emb_in: batch})[0].reshape(len(wins), 96)

    def clip_features(self, x):
        """1 本の音（int16）→ 全部の embedding (n, 96)"""
        return self.embed_windows(self.melspec(x))

    def last_window(self, x):
        """2 秒以上の音の終わりの 1.28 秒 → (16, 96)。短ければ前を 0 で埋める"""
        x = np.asarray(x, np.int16)
        if len(x) < CLIP:
            x = np.concatenate([np.zeros(CLIP - len(x), np.int16), x])
        return self.clip_features(x[-CLIP:])[-N_FEATURES:]


class Stream:
    """マイクの音を少しずつ入れ、80ms ごとに直近 16 個の embedding を返す（openWakeWord の流し方と同じ）"""

    def __init__(self, features):
        self.f = features
        self.raw = np.zeros(0, np.int16)  # melspec に渡す前の音（前の 480 サンプルを残す）
        self.pending = np.zeros(0, np.int16)  # 1280 に満たない端数
        self.spec = np.ones((WINDOW, 32), np.float32)
        # 起動直後も 16 個そろうように、雑音で埋めておく（openWakeWord と同じ）
        rng = np.random.default_rng(0)
        self.feats = self.f.clip_features(rng.integers(-1000, 1000, 16000 * 4).astype(np.int16))

    def push(self, x):
        """音を足す。80ms 分そろうたびに (16, 96) を 1 つずつ返す（0 個のこともある）"""
        x = np.concatenate([self.pending, np.asarray(x, np.int16)])
        n = len(x) // CHUNK * CHUNK
        self.pending = x[n:]
        out = []
        for i in range(0, n, CHUNK):
            chunk = x[i : i + CHUNK]
            self.raw = np.concatenate([self.raw, chunk])[-(CHUNK + 160 * 3) :]
            self.spec = np.vstack([self.spec, self.f.melspec(self.raw)])[-10 * 97 :]
            win = self.spec[-WINDOW:]
            e = self.f.emb.run(None, {self.f.emb_in: win[None, :, :, None].astype(np.float32)})[0].reshape(1, 96)
            self.feats = np.vstack([self.feats, e])[-120:]
            out.append(self.feats[-N_FEATURES:])
        return out


class Head:
    """頭（1536→32→1 の 2 層。numpy の重み）。`train.py` が作り、`listen.py` が使う"""

    def __init__(self, w1, b1, w2, b2, threshold=0.5):
        self.w1, self.b1, self.w2, self.b2 = w1, b1, w2, b2
        self.threshold = float(threshold)

    @classmethod
    def load(cls, path):
        z = np.load(path)
        return cls(z["w1"], z["b1"], z["w2"], z["b2"], float(z["threshold"]))

    def save(self, path, **meta):
        np.savez(path, w1=self.w1, b1=self.b1, w2=self.w2, b2=self.b2, threshold=self.threshold, **meta)

    def scores(self, x):
        """(n, 16, 96) か (16, 96) → 点数 (n,)"""
        x = np.asarray(x, np.float32).reshape(-1, N_FEATURES * 96)
        h = np.maximum(x @ self.w1 + self.b1, 0.0)
        z = (h @ self.w2 + self.b2).reshape(-1)
        return 1.0 / (1.0 + np.exp(-np.clip(z, -30, 30)))
