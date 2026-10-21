"""build_clips.py のテスト。

    python -I -m unittest discover -s scripts -p "test_build_clips.py"

（numpy・scipy・Pillow が要る。py -3 で開く Python 3.13 には入っていないので python で動かす）
"""

import sys
import unittest
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_clips as b  # noqa: E402


def canvas_with_box(l, t, r, bottom, color=(90, 60, 50), fill=(190, 225, 195)):
    """白い 640×640 に、茶色の縁（2px）で囲んだ箱を描く（キャラの代わり）。"""
    rgb = np.full((b.SRC, b.SRC, 3), 252, np.uint8)
    rgb[t:bottom, l:r] = color
    rgb[t + 2 : bottom - 2, l + 2 : r - 2] = fill
    return rgb


class PickTest(unittest.TestCase):
    def test_drops_tail_and_halves_rate(self):
        # 16fps の 81 コマ → 末尾 3 を捨てて 8fps: 0, 2, …, 76（39 コマ）
        idx = b.pick(81, 16, 8, 3)
        self.assertEqual((idx[0], idx[-1], len(idx)), (0, 76, 39))
        # drop_tail 0 なら最後のコマ（80）まで
        self.assertEqual(b.pick(81, 16, 8, 0)[-1], 80)


class RemoveWhiteTest(unittest.TestCase):
    def test_background_becomes_transparent_and_inside_light_stays(self):
        rgb = canvas_with_box(100, 100, 300, 400, fill=(250, 245, 240))  # 中はクリーム（白に近い）
        rgba = b.remove_white(rgb)
        self.assertEqual(rgba[5, 5, 3], 0, "端の白は透明")
        self.assertEqual(rgba[250, 200, 3], 255, "輪郭の内側の明るい色は残る")
        self.assertEqual(b.bbox(rgba), (100, 100, 300, 400))

    def test_small_specks_are_dropped(self):
        rgb = canvas_with_box(100, 100, 300, 400)
        rgb[0, 639] = (237, 235, 235)  # 隅に圧縮のノイズ 1 画素（2026-10-21 実際に起きた）
        rgb[620:623, 10:13] = (200, 200, 200)  # 端に小さなかけら
        self.assertEqual(b.bbox(b.remove_white(rgb)), (100, 100, 300, 400))


class AlignTest(unittest.TestCase):
    def test_first_frame_is_mapped_onto_the_pose(self):
        # 絵が姿勢画像の半分の高さで、横にずれている → 倍率 2、足元と中心が重なる
        s, dx, dy = b.align((200, 300, 300, 400), (70, 119, 570, 600))
        self.assertAlmostEqual(s, 4.81)
        l, t, r, bottom = b.placed_box((200, 300, 300, 400), s, dx, dy)
        self.assertAlmostEqual(bottom, 600)
        self.assertAlmostEqual((l + r) / 2, 320)

    def test_common_scale_fits_every_frame(self):
        # 幅いっぱい（中心から 300px）の絵があると、コマの半分 192px に 98% で収まる倍率になる
        k = b.common_scale([(20, 300, 620, 600)], (384, 416), 404, 1.0)
        self.assertAlmostEqual(k, 0.98 * 192 / 300)
        # 収まるなら基準の倍率のまま
        self.assertEqual(b.common_scale([(220, 400, 420, 600)], (384, 416), 404, 0.7), 0.7)


class PlaceTest(unittest.TestCase):
    def test_floor_and_center_land_on_the_cell(self):
        rgba = b.remove_white(canvas_with_box(220, 120, 420, 600))
        cell = b.place(rgba, 1.0, 0.0, 0.0, 0.5, (384, 416), 404)
        a = np.array(cell)[..., 3]
        ys, xs = np.nonzero(a > 128)
        self.assertLessEqual(abs(int(ys.max()) - 403), 1, "足元が床 y=404 の直上")
        self.assertLessEqual(abs((int(xs.min()) + int(xs.max())) / 2 - 191.5), 1, "横の中心")


if __name__ == "__main__":
    unittest.main()
