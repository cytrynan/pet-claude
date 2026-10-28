"""make-lineup.py のテスト。

    python -I -m unittest discover -s scripts -p "test_make_lineup.py"

（Pillow が要る）
"""

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

from PIL import Image

_spec = importlib.util.spec_from_file_location("make_lineup", Path(__file__).resolve().parent / "make-lineup.py")
assert _spec is not None and _spec.loader is not None
ml = importlib.util.module_from_spec(_spec)
sys.modules["make_lineup"] = ml
_spec.loader.exec_module(ml)


def make_pet(root: Path, pid: str, name: str, color, cell=64):
    d = root / pid
    d.mkdir()
    sheet = Image.new("RGBA", (cell * 8, cell * 2), (0, 0, 0, 0))
    sheet.paste(color + (255,), (10, 10, 50, 60))
    sheet.save(d / "spritesheet.png")
    (d / "pet.json").write_text(
        json.dumps({"id": pid, "displayName": name, "spritesheetPath": "spritesheet.png"}), encoding="utf-8"
    )


class LoadPetsTest(unittest.TestCase):
    def test_orders_known_ids_first_then_folder_order(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            make_pet(root, "zzz-new", "新顔", (10, 10, 200))
            make_pet(root, "rinrin", "りんりん", (200, 10, 10))
            make_pet(root, "mame-dragon", "まめドラゴン", (10, 200, 10))
            (root / "broken").mkdir()
            (root / "broken" / "pet.json").write_text("{", encoding="utf-8")
            pets = ml.load_pets(root)
            self.assertEqual([p[0] for p in pets], ["mame-dragon", "rinrin", "zzz-new"])
            self.assertEqual(pets[0][1], "まめドラゴン")


class ComposeTest(unittest.TestCase):
    def test_lays_pets_in_one_row_with_names(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            make_pet(root, "mame-dragon", "まめドラゴン", (10, 200, 10))
            make_pet(root, "rinrin", "りんりん", (200, 10, 10))
            img = ml.compose(ml.load_pets(root), cell=100)
            # 幅 = 余白 2 + 100×2 + 隙間 1
            self.assertEqual(img.size[0], ml.MARGIN * 2 + 200 + ml.GAP)
            # 1 体目の中央は緑、2 体目の中央は赤（並び順どおり）
            g = img.getpixel((ml.MARGIN + 50, ml.MARGIN + 50))
            r = img.getpixel((ml.MARGIN + 100 + ml.GAP + 50, ml.MARGIN + 50))
            assert isinstance(g, tuple) and isinstance(r, tuple)
            self.assertGreater(g[1], g[0])
            self.assertGreater(r[0], r[1])
            # 名前の帯に暗い画素（文字）がある
            band = img.crop((0, ml.MARGIN + 100, img.size[0], img.size[1])).convert("L")
            self.assertLess(min(band.getdata()), 128)

    def test_rejects_empty(self):
        with self.assertRaises(ValueError):
            ml.compose([])


if __name__ == "__main__":
    unittest.main()
