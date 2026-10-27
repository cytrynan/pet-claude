"""画質の試し（2026-10-27 K 指示）: 同じコマ（待機の 1 コマ目）を、アプリが持つ上限（3/4・等倍）と
生成の大きさ（640・960）の組み合わせで、K の 2 つの画面の窓の大きさに描いたときの見え方を並べる。

    python -I compare_quality.py

アプリの描き方の再現: コマ（384×416）→ 持つ大きさ（窓の大きさ。ただし上限まで）に縮める → 窓がそれより
大きければ双線形で引き伸ばす。縮めるのも双線形（アプリは image クレートの Triangle＝同じ三角フィルタ）。
K の窓: ウルトラワイド（100%・大きさ 150%）= 288×312、ノート（200%・150%）= 576×624。
出力: compare-quality.png（顔の周りを 2 倍に拡大して横に並べる）
"""

from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
CELL = (384, 416)
SOURCES = {"640": ROOT / "assets/pets/rinrin/clips/idle/000.webp", "960": ROOT / "assets/pets/rinrin-960/clips/idle/000.webp"}
WINDOWS = {"wide100 288x312": (288, 312), "note200 576x624": (576, 624)}
CAPS = {"3/4": (288, 312), "full": (384, 416)}


def render(frame: Image.Image, window, cap) -> Image.Image:
    held = (min(window[0], cap[0]), min(window[1], cap[1]))
    im = frame.resize(held, Image.BILINEAR) if held != frame.size else frame
    if window != held:
        im = im.resize(window, Image.BILINEAR)
    return im


def main():
    frames = {k: Image.open(v).convert("RGBA") for k, v in SOURCES.items()}
    cases = []
    for wname, win in WINDOWS.items():
        for cname, cap in CAPS.items():
            for sname, fr in frames.items():
                if win[0] <= cap[0] and cname == "full":
                    continue  # 窓が上限より小さければ 3/4 と同じ絵
                cases.append((f"{wname} / {sname} / 上限 {cname}", render(fr, win, cap), win))
    # 顔のまわり（コマの上 55%）を 2 倍に拡大して並べる。窓の大きさが違うので、同じ実寸で比べられるよう
    # 576 の窓はそのまま、288 の窓は 2 倍にして同じ画面上の大きさ（200% の画面で見た大きさ）にそろえる
    tiles = []
    for label, im, win in cases:
        z = 576 / win[0]
        im = im.resize((round(im.width * z * 2), round(im.height * z * 2)), Image.NEAREST)
        crop = im.crop((im.width * 0.15, im.height * 0.08, im.width * 0.85, im.height * 0.62))
        bg = Image.new("RGB", crop.size, (120, 160, 200))
        bg.paste(crop, (0, 0), crop)
        tiles.append((label, bg))
    w = max(t.width for _, t in tiles)
    h = max(t.height for _, t in tiles)
    sheet = Image.new("RGB", (w * len(tiles), h + 28), "white")
    d = ImageDraw.Draw(sheet)
    for i, (label, t) in enumerate(tiles):
        sheet.paste(t, (i * w, 28))
        d.text((i * w + 6, 6), label, fill="black")
    out = HERE / "compare-quality.png"
    sheet.save(out)
    print(out, sheet.size, [c[0] for c in cases])


if __name__ == "__main__":
    main()
