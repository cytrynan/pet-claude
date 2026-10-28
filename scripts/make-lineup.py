"""同梱の全ペットの待機の絵を名前つきで横一列に並べた 1 枚の PNG を作る（一目で誰がいるか分かる絵）。

    python -I scripts/make-lineup.py assets/pets docs/images/pets-lineup.png

各ペットの `spritesheet.webp` の 1 コマ目（待機の最初の絵。`make-icon.py` と同じ切り出し）を同じ高さにそろえ、
下に `pet.json` の displayName を置く。並びは README の紹介順（`ORDER`）で、知らない id はその後ろに
フォルダ名の順。

なぜ: K 指示（2026-10-28）「すべてのペットの標準の写真を並べて一つの png にして。人目でどんな子が
いるかわかりやすいように」。
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

_spec = importlib.util.spec_from_file_location("make_icon", Path(__file__).resolve().parent / "make-icon.py")
assert _spec is not None and _spec.loader is not None
make_icon = importlib.util.module_from_spec(_spec)
sys.modules["make_icon"] = make_icon
_spec.loader.exec_module(make_icon)

ORDER = ["mame-dragon", "rinrin", "tora", "kuroshiba", "komugi", "goma"]
CELL = 320  # 1 体の絵の正方形（px）
GAP = 24  # 絵と絵の間
MARGIN = 40  # 外側の余白
NAME_PT = 30
FONTS = [r"C:\Windows\Fonts\YuGothB.ttc", r"C:\Windows\Fonts\YuGothM.ttc", r"C:\Windows\Fonts\meiryob.ttc"]
BG = (255, 255, 255, 255)
LABEL = (29, 29, 31, 255)  # apple-design の label


def load_pets(root: Path) -> list[tuple[str, str, Image.Image]]:
    """(id, 表示名, 待機の絵) を紹介順で返す。pet.json かシートが読めないフォルダは飛ばす。"""
    found: dict[str, tuple[str, Image.Image]] = {}
    for d in sorted(p for p in root.iterdir() if p.is_dir()):
        try:
            m = json.loads((d / "pet.json").read_text(encoding="utf-8"))
            sheet = Image.open(d / m.get("spritesheetPath", "spritesheet.webp"))
            art = make_icon.square_art(make_icon.first_cell(sheet), margin=0.0)
        except (OSError, ValueError, KeyError, json.JSONDecodeError):
            continue
        found[m["id"]] = (m.get("displayName", m["id"]), art)
    ids = [i for i in ORDER if i in found] + [i for i in found if i not in ORDER]
    return [(i, found[i][0], found[i][1]) for i in ids]


def font(size: int) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    for f in FONTS:
        try:
            return ImageFont.truetype(f, size)
        except OSError:
            continue
    return ImageFont.load_default()


def compose(pets: list[tuple[str, str, Image.Image]], cell: int = CELL) -> Image.Image:
    """横一列。絵は足元（下端）をそろえ、名前はその下の中央。"""
    if not pets:
        raise ValueError("ペットが 1 体も無い")
    fnt = font(NAME_PT)
    name_h = NAME_PT + 24
    w = MARGIN * 2 + cell * len(pets) + GAP * (len(pets) - 1)
    h = MARGIN * 2 + cell + name_h
    out = Image.new("RGBA", (w, h), BG)
    draw = ImageDraw.Draw(out)
    for i, (_, name, art) in enumerate(pets):
        x = MARGIN + i * (cell + GAP)
        pic = art.resize((cell, cell), Image.Resampling.LANCZOS)
        out.paste(pic, (x, MARGIN), pic)
        tw = draw.textlength(name, font=fnt)
        draw.text((x + (cell - tw) / 2, MARGIN + cell + 12), name, font=fnt, fill=LABEL)
    return out


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="全ペットの待機の絵を名前つきで 1 枚に並べる")
    ap.add_argument("pets_dir", type=Path, help="assets/pets")
    ap.add_argument("out", type=Path, help="書き出す PNG")
    a = ap.parse_args(argv)
    pets = load_pets(a.pets_dir)
    img = compose(pets)
    a.out.parent.mkdir(parents=True, exist_ok=True)
    img.save(a.out, optimize=True)
    print(f"wrote {a.out} {img.size[0]}x{img.size[1]} pets={[p[0] for p in pets]}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
