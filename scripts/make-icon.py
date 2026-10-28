"""ペットのスプライトシートの最初のコマ（待機の 1 コマ目）からアプリのアイコンを作る。

    python -I scripts/make-icon.py assets/pets/mame-dragon/spritesheet.webp crates/pet-app/icons

出力は `icon.png`（256px）と `icon.ico`（256・128・64・48・32・16）。exe のアイコン（tauri-build が
`crates/pet-app/icons/icon.ico` を埋め込む）、トレイ、デスクトップのショートカットの 3 つが同じ絵になる。

なぜ: リリース配布（2026-10-28 K 指示）で、デスクトップのショートカットをまめドラゴンの絵にする。
仮キャラの絵（`pet_core::placeholder::generate_icon`）はペットと関係が無いので置き換える。

決め方:
- シートは Codex ペット互換（8 列・行ごとに 1 ポーズ・コマは正方形）。1 行目の 1 コマ目が待機の最初の絵
- 不透明な部分の外接矩形で切り、長い辺に合わせた正方形の中央に置く。余白は辺の 4%（アイコンの枠いっぱいだと
  Windows の角丸と重なって端が欠けて見える）
- 縮小は LANCZOS。ICO の各サイズは 256 の PNG から個別に縮める（Pillow に任せると 256 を箱で縮めて
  16px がにじむ）
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from PIL import Image

COLUMNS = 8
MARGIN = 0.04
PNG_SIZE = 256
ICO_SIZES = (256, 128, 64, 48, 32, 16)


def first_cell(sheet: Image.Image, columns: int = COLUMNS) -> Image.Image:
    """1 行目の 1 コマ目（正方形）を切り出す。"""
    cell = sheet.width // columns
    if cell == 0 or sheet.height < cell:
        raise ValueError(f"シートが小さすぎる: {sheet.size}")
    return sheet.convert("RGBA").crop((0, 0, cell, cell))


def square_art(cell: Image.Image, margin: float = MARGIN) -> Image.Image:
    """不透明な部分を正方形の中央に置き、周りに `margin` の余白を取る（透明）。"""
    bbox = cell.getbbox()
    if bbox is None:
        raise ValueError("コマが全部透明")
    art = cell.crop(bbox)
    side = max(art.width, art.height)
    canvas = int(round(side / (1 - 2 * margin)))
    out = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
    out.paste(art, ((canvas - art.width) // 2, (canvas - art.height) // 2), art)
    return out


def render(square: Image.Image, size: int) -> Image.Image:
    return square.resize((size, size), Image.Resampling.LANCZOS)


def write_icons(sheet_path: Path, out_dir: Path) -> tuple[Path, Path]:
    square = square_art(first_cell(Image.open(sheet_path)))
    base = render(square, PNG_SIZE)
    out_dir.mkdir(parents=True, exist_ok=True)
    png = out_dir / "icon.png"
    ico = out_dir / "icon.ico"
    base.save(png)
    frames = [base] + [render(square, s) for s in ICO_SIZES if s != PNG_SIZE]
    # append_images に渡した絵をそのままの大きさで入れる（sizes に同じ寸法を並べると Pillow は縮めない）
    base.save(ico, format="ICO", sizes=[(s, s) for s in ICO_SIZES], append_images=frames[1:])
    return png, ico


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="スプライトシートの 1 コマ目からアプリのアイコンを作る")
    ap.add_argument("sheet", type=Path, help="スプライトシート（webp/png）")
    ap.add_argument("out_dir", type=Path, help="icon.png と icon.ico を書く場所")
    a = ap.parse_args(argv)
    png, ico = write_icons(a.sheet, a.out_dir)
    print(f"wrote {png} {ico}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
