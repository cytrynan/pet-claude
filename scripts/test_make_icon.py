"""make-icon.py のテスト。

    python -I -m unittest discover -s scripts -p "test_make_icon.py"

（Pillow が要る）
"""

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

from PIL import IcoImagePlugin, Image

_spec = importlib.util.spec_from_file_location("make_icon", Path(__file__).resolve().parent / "make-icon.py")
assert _spec is not None and _spec.loader is not None
mi = importlib.util.module_from_spec(_spec)
sys.modules["make_icon"] = mi
_spec.loader.exec_module(mi)


def sheet_with_blob(cell=64, columns=8, rows=2, box=(10, 20, 40, 60)):
    """透明なシートの 1 コマ目に赤い箱（キャラの代わり）、2 コマ目に緑の箱を置く。"""
    sheet = Image.new("RGBA", (cell * columns, cell * rows), (0, 0, 0, 0))
    l, t, r, b = box
    sheet.paste((220, 40, 40, 255), (l, t, r, b))
    sheet.paste((40, 220, 40, 255), (cell + 5, 5, cell + 30, 30))
    return sheet


class FirstCellTest(unittest.TestCase):
    def test_takes_only_the_first_cell(self):
        cell = mi.first_cell(sheet_with_blob())
        self.assertEqual(cell.size, (64, 64))
        # 2 コマ目の緑は入らない
        self.assertEqual(cell.getpixel((5, 5))[3], 0)
        self.assertEqual(cell.getpixel((20, 40)), (220, 40, 40, 255))

    def test_rejects_a_sheet_shorter_than_one_cell(self):
        with self.assertRaises(ValueError):
            mi.first_cell(Image.new("RGBA", (80, 5)))


class SquareArtTest(unittest.TestCase):
    def test_centers_the_art_with_margin(self):
        sq = mi.square_art(mi.first_cell(sheet_with_blob()), margin=0.1)
        # 箱は 30×40 → 長辺 40 を 80% に置く → 50px の正方形
        self.assertEqual(sq.size, (50, 50))
        self.assertEqual(sq.getpixel((25, 25)), (220, 40, 40, 255))
        # 四隅と上下の余白は透明
        for p in [(0, 0), (49, 49), (25, 2), (25, 47)]:
            self.assertEqual(sq.getpixel(p)[3], 0, p)

    def test_rejects_an_empty_cell(self):
        with self.assertRaises(ValueError):
            mi.square_art(Image.new("RGBA", (64, 64), (0, 0, 0, 0)))


class WriteIconsTest(unittest.TestCase):
    def test_writes_png_and_multi_size_ico(self):
        with tempfile.TemporaryDirectory() as d:
            sheet = Path(d) / "sheet.png"
            sheet_with_blob().save(sheet)
            png, ico = mi.write_icons(sheet, Path(d) / "out")
            self.assertEqual(Image.open(png).size, (256, 256))
            with Image.open(ico) as im:
                self.assertEqual(im.format, "ICO")
                sizes = {s for s in im.info["sizes"]}
            self.assertEqual(sizes, {(s, s) for s in mi.ICO_SIZES})
            # 16px の絵も中央に赤が残る（縮小で消えていない）
            with Image.open(ico) as im:
                assert isinstance(im, IcoImagePlugin.IcoImageFile)
                small = im.ico.getimage((16, 16)).convert("RGBA")
            self.assertEqual(small.size, (16, 16))
            px = small.getpixel((8, 8))
            assert isinstance(px, tuple)
            self.assertGreater(px[3], 200)


if __name__ == "__main__":
    unittest.main()
