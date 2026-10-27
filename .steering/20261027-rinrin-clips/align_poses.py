"""りんりん（パンダ）の状態別静止画を、Wan 2.2 FLF2V に渡す姿勢画像（640×640 白背景）にそろえる。

    python -I .steering/20261027-rinrin-clips/align_poses.py

まめドラゴンの `../20261021-wan-transitions/align_poses.py` を流用した。違い:
- 元素材は zip（`src/`）の 8 枚。zip の時点で頭の幅（約 560px）と床（y=900）がそろっている
  （zip の README・`src/zip-verification.json`）ので、ここでは共通倍率 1 つで 640 に縮め、
  待機の高さを 480 にそろえる（まめドラゴンと同じ基準。`scale = 480 / 待機の高さ`）。
- 歩きのコマ（walk00）は無い。歩きのループ・止まる・歩き出すの始点/終点は `02-walk.png`（`walk.png`）を使う。
- 床: 立ち・座り・寝姿は y=600。喜ぶは zip で床から 60px 浮いている（下端 840）ので 560。
  つままれるは、まめドラゴンと同じ規約で 560（ぶら下がり）。
出力 `poses/` の bbox は `verification.json` に書く（`scripts/build_clips.py` が 1 コマ目をこれに合わせる）。
bbox の right/bottom は含まない（exclusive）。
"""

import hashlib
import json
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageOps

ROOT = Path(__file__).resolve().parent
# 引数で大きさを変えられる（既定 640。2026-10-27 K 指示の画質の試しで 960 を作る）。640 以外は poses<大きさ>/ に出す
SIZE = int(sys.argv[1]) if len(sys.argv) > 1 else 640
R = SIZE / 640  # 640 基準の座標（床 600・中心 320・待機 480）をこの倍率で伸ばす
OUT = ROOT / ("poses" if SIZE == 640 else f"poses{SIZE}")
SRC = ROOT / "src"
FILES = {
    "idle": SRC / "01-idle.png",
    "walk": SRC / "02-walk.png",
    "working": SRC / "03-working.png",
    "attention": SRC / "04-attention.png",
    "happy": SRC / "05-happy.png",
    "confused": SRC / "06-confused.png",
    "sleeping": SRC / "07-sleeping.png",
    "dragged": SRC / "08-dragged.png",
}
# 動画の始点・終点に使う順（contact.png の並び）
ORDER = ["idle", "idle_right", "walk", "working", "working_right", "attention", "happy",
         "confused", "sleeping", "sleeping_right", "dragged"]
# 待機の高さ（まめドラゴンと同じ基準。640 のキャンバスで足元 600・頭が 120 付近）
IDLE_HEIGHT = round(480 * R)
# 下端を置く y。喜ぶは zip で 60px 浮いている（1024 基準）、つままれるはぶら下がり
FLOOR = {"happy": round(560 * R), "dragged": round(560 * R)}
FLOOR_DEFAULT = round(600 * R)
CENTER = round(320 * R)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def measured_bbox(im):
    """保存後の RGB 画像で、どれかの色が 250 未満の範囲（build_clips の姿勢合わせと同じ測り方）。"""
    channels = [c.point(lambda v: 255 if v < 250 else 0) for c in im.split()]
    return ImageChops.lighter(ImageChops.lighter(*channels[:2]), channels[2]).getbbox()


def main():
    if OUT.exists():
        raise FileExistsError(f"Refusing to overwrite existing output: {OUT}")
    hashes = {name: digest(path) for name, path in FILES.items()}
    images, boxes = {}, {}
    for name, path in FILES.items():
        with Image.open(path) as src:
            images[name] = src.convert("RGBA")
        boxes[name] = images[name].getchannel("A").point(lambda a: 255 if a > 16 else 0).getbbox()
        if boxes[name] is None:
            raise ValueError(f"Empty alpha bbox: {path}")
    heights = {k: b[3] - b[1] for k, b in boxes.items()}
    scale = IDLE_HEIGHT / heights["idle"]
    results, metadata = {}, {}
    for name, src in images.items():
        left, top, right, bottom = boxes[name]
        size = (round(src.width * scale), round(src.height * scale))
        resized = src.resize(size, Image.Resampling.LANCZOS)
        sx, sy = size[0] / src.width, size[1] / src.height
        floor = FLOOR.get(name, FLOOR_DEFAULT)
        offset = (round(CENTER - (left + right) / 2 * sx), round(floor - bottom * sy))
        canvas = Image.new("RGBA", (SIZE, SIZE), (255, 255, 255, 255))
        canvas.alpha_composite(resized, dest=offset)
        results[name] = canvas.convert("RGB")
        metadata[name] = {"source": str(FILES[name]), "scale": scale,
                          "source_alpha_bbox": boxes[name], "resized_size": size,
                          "offset": offset, "target_bottom_y": floor}
        if name in ("idle", "working", "sleeping"):
            results[name + "_right"] = ImageOps.mirror(results[name])
            metadata[name + "_right"] = {**metadata[name], "mirrored_from": name + ".png"}
    OUT.mkdir()
    records = {}
    for name in ORDER:
        path = OUT / (name + ".png")
        results[name].save(path)
        with Image.open(path) as saved:
            bbox = measured_bbox(saved)
            records[path.name] = {**metadata[name], "size": list(saved.size), "mode": saved.mode,
                                  "bbox": bbox, "bbox_height": bbox[3] - bbox[1],
                                  "bottom_y": bbox[3], "center_x": (bbox[0] + bbox[2]) / 2}
    contact = Image.new("RGB", (SIZE * len(ORDER), SIZE), "white")
    draw = ImageDraw.Draw(contact)
    for index, name in enumerate(ORDER):
        contact.paste(results[name], (index * SIZE, 0))
        draw.text((index * SIZE + 10, 10), name + ".png", fill="black")
    draw.line((0, FLOOR_DEFAULT, contact.width - 1, FLOOR_DEFAULT), fill=(255, 0, 0), width=1)
    contact.save(OUT / "contact.png")
    unchanged = all(digest(path) == hashes[name] for name, path in FILES.items())

    def floor_ok(n, r):
        f = FLOOR.get(n.removesuffix(".png"), FLOOR_DEFAULT)
        return f - 2 <= r["bottom_y"] <= f + 2

    checks = {
        "11_rgb_640": len(records) == 11 and all(r["size"] == [SIZE, SIZE] and r["mode"] == "RGB" for r in records.values()),
        "alignment": all(floor_ok(n, r) and CENTER - 2 <= r["center_x"] <= CENTER + 2 for n, r in records.items()),
        "idle_height": records["idle.png"]["bbox_height"],
        "inputs_unchanged_sha256": unchanged,
    }
    report = {"size": SIZE, "floor": FLOOR_DEFAULT, "center": CENTER,
              "bbox_convention": "[left, top, right exclusive, bottom exclusive]",
              "measurement": "any RGB channel < 250", "outputs": records,
              "input_sha256": hashes, "checks": checks}
    (OUT / "verification.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps({"checks": checks, **{n: (r["bbox"], r["bottom_y"], r["center_x"]) for n, r in records.items()}}, indent=1))
    assert checks["11_rgb_640"] and checks["alignment"] and unchanged
    assert abs(checks["idle_height"] - IDLE_HEIGHT) <= 2


if __name__ == "__main__":
    main()
