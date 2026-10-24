"""ウェイクワードの道具のうち numpy が要る部分のテスト（onnxruntime と openWakeWord のモデルは要らない）。

    python -I -m unittest discover -s scripts/wake -p "test_wake_np.py"

ゲートは声の環境（%LOCALAPPDATA%\\pet-claude\\stt\\venv）の python で、CI は numpy を入れて走らせる。
特徴抽出（melspectrogram.onnx・embedding_model.onnx）は形だけ同じ偽物に差し替える。本物で測るのは evaluate.py。
"""

import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
import wave
from contextlib import redirect_stdout
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import audio  # noqa: E402
import common  # noqa: E402
import evaluate  # noqa: E402
import oww  # noqa: E402
import split  # noqa: E402
import train  # noqa: E402


class FakeEmb:
    """embedding_model.onnx の代わり: (n, 76, 32, 1) → (n, 1, 1, 96)。窓の平均を 96 個並べる"""

    def run(self, _outputs, feeds):
        x = next(iter(feeds.values()))
        assert x.shape[1:] == (oww.WINDOW, 32, 1), x.shape
        return [np.repeat(x.mean(axis=(1, 2, 3))[:, None], 96, axis=1).reshape(len(x), 1, 1, 96)]


class FakeFeatures(oww.Features):
    """melspectrogram.onnx の代わり: 本物と同じフレームの数え方（n_fft 512・ずらし 160・端を足さない。
    1280+480 サンプルで 8 フレーム、2 秒で 197 フレーム＝embedding 16 個）。値は窓の平均"""

    def __init__(self):
        self.emb = FakeEmb()
        self.emb_in = "x"

    def melspec(self, x):
        x = np.asarray(x, np.int16).astype(np.float32)
        n = max(0, (len(x) - 512) // 160 + 1)
        return np.stack([np.full(32, x[i * 160 : i * 160 + 512].mean() / 1000) for i in range(n)]).reshape(n, 32)


def write_wav(path, x, rate=16000, channels=1):
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as w:
        w.setnchannels(channels)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(np.asarray(x, "<i2").tobytes())


class AudioTest(unittest.TestCase):
    def test_load_wav_makes_16k_mono(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "a.wav"
            # 8kHz・ステレオ 1 秒（左 1000・右 3000）→ 16kHz・モノラル 2 倍の長さ・平均 2000
            write_wav(p, np.tile([1000, 3000], 8000), rate=8000, channels=2)
            x = audio.load_wav(p)
            self.assertEqual(x.dtype, np.int16)
            self.assertEqual(len(x), 16000)
            self.assertTrue(np.all(x == 2000))

    def test_trim_drops_silence_but_keeps_padding(self):
        x = np.zeros(10000, np.int16)
        x[4000:5000] = 3000
        y = audio.trim(x, level=500, pad=800)
        self.assertEqual(len(y), 1000 + 2 * 800 - 1)
        self.assertEqual(len(audio.trim(np.zeros(100, np.int16))), 0)

    def test_noise_has_unit_rms(self):
        rng = np.random.default_rng(0)
        for color in ["white", "pink", "brown"]:
            self.assertAlmostEqual(audio.rms(audio.noise(16000, rng, color)), 1.0, places=3)

    def test_place_at_end_puts_the_word_near_the_end(self):
        rng = np.random.default_rng(1)
        word = np.full(4000, 10000, np.float32)
        for _ in range(20):
            out = audio.place_at_end(word, oww.CLIP, rng, max_gap=0.2, snr=(40, 40))
            self.assertEqual((out.dtype, len(out)), (np.int16, oww.CLIP))
            loud = np.flatnonzero(np.abs(out.astype(np.int32)) > 5000)
            # 語の終わりは窓の終わりから 0.2 秒以内、長さは語のまま
            self.assertGreaterEqual(loud[-1], oww.CLIP - 1 - int(0.2 * audio.RATE))
            self.assertEqual(loud[-1] - loud[0] + 1, len(word))

    def test_mix_background_snr(self):
        rng = np.random.default_rng(2)
        x = np.full(16000, 1000, np.float32)
        y = audio.mix_background(x, rng, snr=(20, 20)).astype(np.float32)
        # 混ぜた雑音の RMS は信号の 1/10（20dB）
        self.assertAlmostEqual(audio.rms(y - x), 100.0, delta=2.0)


class SplitTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.old = os.environ.get("PET_CLAUDE_DATA_DIR")
        os.environ["PET_CLAUDE_DATA_DIR"] = self.tmp.name
        d = common.pet_data("p")
        tone = (np.sin(np.arange(8000) / 5) * 8000).astype(np.int16)
        for kind, key, values in [("synth-pos", "speaker", [f"s{i}" for i in range(30)]),
                                  ("synth-neg", "text", [f"文{i}" for i in range(30)])]:
            rows = []
            for i, v in enumerate(values):
                name = f"{kind}-{i}.wav"
                write_wav(d / kind / name, tone)
                rows.append({"file": name, "speaker": v if key == "speaker" else "s", "text": v if key == "text" else "まめちゃん"})
            rows.append({"file": "missing.wav", "speaker": "s0", "text": "文0"})  # 消えたファイルは読まない
            (d / kind / "manifest.jsonl").write_text("\n".join(json.dumps(r, ensure_ascii=False) for r in rows), encoding="utf-8")
        for i in range(10):
            write_wav(d / "k" / f"word-{i:02d}.wav", tone)
        write_wav(d / "k" / "room-01.wav", tone)
        np.save(common.wake_dir() / "validation_set_features.npy", np.arange(100 * 96, dtype=np.float32).reshape(100, 96))

    def tearDown(self):
        if self.old is None:
            os.environ.pop("PET_CLAUDE_DATA_DIR", None)
        else:
            os.environ["PET_CLAUDE_DATA_DIR"] = self.old
        self.tmp.cleanup()

    def test_train_and_eval_are_disjoint_and_cover_everything(self):
        tr, ev = split.load("p", "train"), split.load("p", "eval")
        self.assertEqual(set(tr["pos_speakers"]) & set(ev["pos_speakers"]), set())
        self.assertEqual(len(tr["pos"]) + len(ev["pos"]), 30)
        self.assertEqual(set(tr["neg_texts"]) & set(ev["neg_texts"]), set())
        self.assertEqual(len(tr["neg"]) + len(ev["neg"]), 30)
        self.assertTrue(ev["pos"] and ev["neg"], "crc で判定用が 1 本もない＝分け方が壊れている")
        # K の録音は後ろの 3 割が判定へ。部屋の音は両方の背景
        self.assertEqual((len(tr["k_pos"]), len(ev["k_pos"])), (7, 3))
        self.assertEqual((len(tr["k_room"]), len(ev["k_room"])), (1, 1))
        # 公開データは前半と後半
        self.assertEqual(tr["val"][-1, 0] + 96, ev["val"][0, 0])
        self.assertEqual(tr["val"].shape[0] + ev["val"].shape[0], 100)

    def test_split_is_stable(self):
        self.assertEqual(split.is_eval_speaker("Amene Lila"), split.is_eval_speaker("Amene Lila"))
        self.assertEqual(split._tail_split(["c", "a", "b"], "eval"), ["c"])
        self.assertEqual(split._tail_split(["a", "b"], "eval"), [])  # 2 本以下は全部学習へ


class StreamTest(unittest.TestCase):
    def test_windows_and_padding(self):
        f = FakeFeatures()
        spec = np.zeros((100, 32), np.float32)
        self.assertEqual(f.embed_windows(spec).shape, ((100 - oww.WINDOW) // oww.STEP + 1, 96))
        self.assertEqual(f.embed_windows(spec[:10]).shape, (0, 96))
        self.assertEqual(f.last_window(np.zeros(1000, np.int16)).shape, (oww.N_FEATURES, 96))

    def test_stream_emits_one_window_per_80ms_and_keeps_the_remainder(self):
        s = oww.Stream(FakeFeatures())
        out = s.push(np.zeros(oww.CHUNK * 3 + 100, np.int16))
        self.assertEqual(len(out), 3)
        self.assertTrue(all(o.shape == (oww.N_FEATURES, 96) for o in out))
        self.assertEqual(len(s.pending), 100)
        # 端数は次に足される: 1180 足すと 1280 になって 1 つ
        self.assertEqual(len(s.push(np.zeros(oww.CHUNK - 100, np.int16))), 1)
        self.assertEqual(len(s.pending), 0)

    def test_stream_spectrogram_matches_offline(self):
        # 80ms ずつ流して積んだ melspec の直近 76 フレームが、一括で作った物の終わり 76 フレームと同じ
        # （前に 480 サンプル足す流し方で、フレームが重ならず抜けもしない＝学習と検出が同じ特徴の上にある）
        f = FakeFeatures()
        x = (np.random.default_rng(3).standard_normal(oww.CHUNK * 40) * 3000).astype(np.int16)
        s = oww.Stream(f)
        for i in range(0, len(x), oww.CHUNK):
            s.push(x[i : i + oww.CHUNK])
        offline = f.melspec(np.concatenate([np.zeros(480, np.int16), x]))
        self.assertEqual(offline.shape[0], 8 * 40)
        np.testing.assert_allclose(s.spec[-oww.WINDOW :], offline[-oww.WINDOW :], rtol=1e-5)
        self.assertEqual(f.last_window(x[: oww.CLIP]).shape, (oww.N_FEATURES, 96))
        self.assertEqual(f.clip_features(x[: oww.CLIP]).shape[0], oww.N_FEATURES)  # 2 秒でちょうど 16 個


class HeadTest(unittest.TestCase):
    def test_windows_of(self):
        feats = np.arange(30 * 96, dtype=np.float32).reshape(30, 96)
        w = train.windows_of(feats, step=2)
        self.assertEqual(w.shape, (8, 16, 96))
        np.testing.assert_array_equal(w[1], feats[2:18])
        self.assertEqual(train.windows_of(feats[:10]).shape, (0, 16, 96))

    def test_mlp_learns_a_separable_problem_and_round_trips(self):
        rng = np.random.default_rng(0)
        X = rng.standard_normal((600, oww.N_FEATURES * 96)).astype(np.float32)
        y = (X[:, :50].sum(axis=1) > 0).astype(np.float32)
        w = np.ones_like(y)
        with redirect_stdout(io.StringIO()):
            head = train.train_mlp(X, y, w, epochs=40, seed=0)
        acc = ((head.scores(X) > 0.5) == (y == 1)).mean()
        self.assertGreater(acc, 0.95)
        head.threshold = 0.4
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "h.npz"
            head.save(p)
            back = oww.Head.load(p)
        self.assertEqual(back.threshold, 0.4)
        np.testing.assert_allclose(back.scores(X[:5].reshape(5, 16, 96)), head.scores(X[:5]), rtol=1e-6)


class EvaluateTest(unittest.TestCase):
    def test_rates(self):
        hit = [np.array([0.0, 0.9, 0.9]), np.array([0.0, 0.3, 0.3])]
        rates = evaluate.hit_rates(hit)
        self.assertEqual((rates[0.25], rates[0.5]), (1.0, 0.5))
        self.assertIsNone(evaluate.hit_rates([])[0.5])
        # 30 分で 2 回 → 4 回/時
        fa = evaluate.fa_per_hour([np.array([0.9, 0.9] + [0.0] * 30 + [0.9, 0.9])], 1800)
        self.assertEqual(fa[0.5], 4.0)

    def _rows(self, k_hit, synth_hit, pub, ja):
        return {"k_hit": {t: k_hit(t) for t in evaluate.THRESHOLDS},
                "synth_hit": {t: synth_hit(t) for t in evaluate.THRESHOLDS},
                "public_fa_per_hour": {t: pub(t) for t in evaluate.THRESHOLDS},
                "ja_fa_per_hour": {t: ja(t) for t in evaluate.THRESHOLDS}}

    def test_choose_threshold_respects_both_false_alarm_limits(self):
        rows = self._rows(lambda t: 1.0 - t, lambda t: 0.0,
                          lambda t: 0.4 if t >= 0.3 else 2.0,  # 公開: 0.3 以上で 0.5 回/時以下
                          lambda t: 5.0 if t >= 0.5 else 9.0)  # 会話: 0.5 以上で 10 分に 1 回以下
        self.assertEqual(evaluate.choose_threshold(rows, max_fa=0.5), 0.5)

    def test_choose_threshold_prefers_the_lower_on_ties_and_falls_back(self):
        rows = self._rows(lambda t: 1.0, lambda t: 0.0, lambda t: 0.0, lambda t: 0.0)
        self.assertEqual(evaluate.choose_threshold(rows, 0.5), evaluate.THRESHOLDS[0])
        never = self._rows(lambda t: 1.0, lambda t: 1.0, lambda t: 99.0, lambda t: 0.0)
        self.assertEqual(evaluate.choose_threshold(never, 0.5), max(evaluate.THRESHOLDS))

    def test_choose_threshold_uses_synth_when_k_is_missing(self):
        rows = self._rows(lambda t: None, lambda t: 1.0 if t == 0.7 else 0.5, lambda t: 0.0, lambda t: 0.0)
        self.assertEqual(evaluate.choose_threshold(rows, 0.5), 0.7)


class ListenTest(unittest.TestCase):
    def test_missing_head_is_one_json_line_and_exit_2(self):
        with tempfile.TemporaryDirectory() as d:
            env = dict(os.environ, PET_CLAUDE_DATA_DIR=d, PYTHONIOENCODING="utf-8")
            r = subprocess.run([sys.executable, "-I", str(HERE / "listen.py"), "--pet", "none"],
                               input=b"", capture_output=True, env=env, timeout=60)
        self.assertEqual(r.returncode, 2, r.stderr)
        lines = r.stdout.decode("ascii").splitlines()
        self.assertEqual(len(lines), 1)
        self.assertEqual(json.loads(lines[0])["event"], "error")


if __name__ == "__main__":
    unittest.main()
