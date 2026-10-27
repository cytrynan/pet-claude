"""撮った画像をマニュアル用に整える（2026-10-28）。

- 実機の撮影（capture-shots.sh の出力）は、下敷きの無地（236,236,240）の余白を切り落とし、周りに 24px 残す
- Edge の描画（capture-ui.js の出力）はそのまま
- 1 枚 500KB 以下にする（超えたら 256 色に減色し、それでも超えたら縮める）

使い方: python docs/manual/finish-images.py <実機の撮影フォルダ> <Edge の描画フォルダ> <出力フォルダ>
"""
import io
import sys
from pathlib import Path

from PIL import Image, ImageChops

BACKDROP = (236, 236, 240)
LIMIT = 500 * 1024
MARGIN = 24
# 実機の撮影 → 出力名
SHOTS = {
    "idle": "pet-idle",
    "bar": "pet-bar",
    "menu": "pet-menu",
    "perm": "perm-card",
    "ask": "ask-card",
    "status": "status-bubble",
}
# Edge の描画 → 出力名（同じ名前）
RENDERS = ["settings", "chat-empty", "chat-answer", "chat-menu-mode", "chat-menu-model"]


def autocrop(im: Image.Image) -> Image.Image:
    """下敷きの色と違う画素の範囲に余白を足して切り出す。差が小さい画素（影の裾）も中身に含める。"""
    rgb = im.convert("RGB")
    diff = ImageChops.difference(rgb, Image.new("RGB", rgb.size, BACKDROP)).convert("L")
    bbox = diff.point(lambda v: 255 if v > 6 else 0).getbbox()
    if not bbox:
        return rgb
    l, t, r, b = bbox
    return rgb.crop((max(0, l - MARGIN), max(0, t - MARGIN), min(rgb.width, r + MARGIN), min(rgb.height, b + MARGIN)))


def save_under_limit(im: Image.Image, out: Path) -> int:
    """500KB 以下になるまで、減色 → 縮小の順で落とす。戻り値は書いたバイト数。"""
    candidates = [im, im.convert("RGB").quantize(colors=256, method=Image.Quantize.MEDIANCUT)]
    for cand in candidates:
        buf = io.BytesIO()
        cand.save(buf, "PNG", optimize=True)
        if buf.tell() <= LIMIT:
            out.write_bytes(buf.getvalue())
            return buf.tell()
    small = im
    while True:
        small = small.resize((int(small.width * 0.85), int(small.height * 0.85)), Image.LANCZOS)
        buf = io.BytesIO()
        small.convert("RGB").quantize(colors=256).save(buf, "PNG", optimize=True)
        if buf.tell() <= LIMIT:
            out.write_bytes(buf.getvalue())
            return buf.tell()


def main() -> None:
    shots, renders, out = (Path(p) for p in sys.argv[1:4])
    out.mkdir(parents=True, exist_ok=True)
    for src, name in SHOTS.items():
        f = shots / f"{src}.png"
        if not f.is_file():
            print(f"skip {f}（無い）")
            continue
        n = save_under_limit(autocrop(Image.open(f)), out / f"{name}.png")
        print(f"{name}.png {n // 1024}KB")
    for name in RENDERS:
        f = renders / f"{name}.png"
        if not f.is_file():
            print(f"skip {f}（無い）")
            continue
        n = save_under_limit(Image.open(f), out / f"{name}.png")
        print(f"{name}.png {n // 1024}KB")


if __name__ == "__main__":
    main()
