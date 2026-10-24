"""ウェイクワードの道具のうち numpy の要らない部分のテスト（ゲートから py -3 で走らせる）。

    py -3 -I -m unittest discover -s scripts/wake -p "test_wake.py"
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import detect  # noqa: E402
import texts  # noqa: E402


class GateTest(unittest.TestCase):
    def test_needs_consecutive_frames(self):
        g = detect.Gate(0.5, consecutive=2, refractory=0)
        self.assertEqual([g.push(s) for s in [0.9, 0.1, 0.9, 0.9]], [False, False, False, True])

    def test_threshold_is_inclusive(self):
        g = detect.Gate(0.5, consecutive=1, refractory=0)
        self.assertTrue(g.push(0.5))

    def test_refractory_suppresses_the_tail_of_the_same_word(self):
        # 語の終わりの後も点数が高いまま 10 フレーム残る → 1 回だけ
        self.assertEqual(detect.count_events([0.9] * 12, 0.5, consecutive=2, refractory=25), 1)

    def test_fires_again_after_refractory(self):
        scores = [0.9, 0.9] + [0.0] * 25 + [0.9, 0.9]
        self.assertEqual(detect.count_events(scores, 0.5, consecutive=2, refractory=25), 2)
        # 冷却が 1 フレーム足りなければ 2 回目の 1 フレーム目が食われて 1 回
        scores = [0.9, 0.9] + [0.0] * 24 + [0.9, 0.9]
        self.assertEqual(detect.count_events(scores, 0.5, consecutive=2, refractory=25), 1)

    def test_rejects_bad_parameters(self):
        for args in [(0.0,), (1.0,), (0.5, 0), (0.5, 1, -1)]:
            with self.assertRaises(ValueError):
                detect.Gate(*args)


class TextsTest(unittest.TestCase):
    def test_morae_keeps_small_kana_with_previous(self):
        self.assertEqual(texts.morae("まめちゃん"), ["ま", "め", "ちゃ", "ん"])
        self.assertEqual(texts.morae("きょうっ"), ["きょ", "う", "っ"])

    def test_near_misses_exclude_the_word_and_are_unique(self):
        near = texts.near_misses("まめちゃん")
        self.assertNotIn("まめちゃん", near)
        self.assertEqual(len(near), len(set(near)))
        for expected in ["ままちゃん", "まめしゃん", "まめ", "まめくん", "めちゃん"]:
            self.assertIn(expected, near)

    def test_near_misses_are_deterministic_and_limited(self):
        self.assertEqual(texts.near_misses("まめちゃん"), texts.near_misses("まめちゃん"))
        self.assertLessEqual(len(texts.near_misses("ぽちぽちくん", limit=10)), 10)

    def test_positives_end_with_the_word(self):
        for p in texts.positives("まめちゃん"):
            self.assertTrue("まめちゃ" in p, p)
        self.assertIn("まめちゃーん", texts.positives("まめちゃん"))

    def test_sentences_never_contain_the_word(self):
        for s in texts.SENTENCES:
            self.assertNotIn("まめちゃん", s)


if __name__ == "__main__":
    unittest.main()
