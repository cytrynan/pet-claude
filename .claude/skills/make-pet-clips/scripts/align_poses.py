"""状態別の静止画（透過 PNG）を、Wan 2.2 FLF2V に渡す姿勢画像（正方形・白背景・床と中心をそろえた）にする。

    python -I align_poses.py <静止画のフォルダ> <出力フォルダ> [--size 960] [--floor happy=560] [--walk-frame <png>]

- 静止画はファイル名に状態の名前（idle・walk・working・attention・happy・confused・sleeping・dragged）を含む
  （build_pet と同じ規約）。8 枚そろっていなくても、あるものだけ作る。
- 全部を 1 つの倍率で縮め（待機の高さを 480×size/640 に）、足元を床（600×size/640）、横の中心を size/2 に置く。
  床を変えたい状態は --floor 名前=px（640 基準の px。つままれるは既定で 560＝ぶら下がり）。
- --walk-frame は、描いた歩きのコマ（まめドラゴンの walk00）を歩行の始点/終点にするとき。高さは歩行の静止画に合わせる。
- 反転版（idle_right・working_right・sleeping_right）も作る。出力の bbox は verification.json に書き、
  scripts/build_clips.py が 1 コマ目をこれに合わせる。bbox の right/bottom は含まない。

なぜこの形か: 姿勢画像の床・中心・倍率がそろっていないと、つなぎで体が跳ねる（2026-10-21 まめドラゴン）。
"""

import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageOps

STATES = ["idle", "walk", "working", "attention", "happy", "confused", "sleeping", "dragged"]
# 動画の始点・終点に使う順（contact.png の並び）
ORDER = ["idle", "idle_right", "walk", "working", "working_right", "attention", "happy",
         "confused", "sleeping", "sleeping_right", "dragged"]
MIRRORED = ("idle", "working", "sleeping")
# 640 基準の規約: 待機の高さ 480・床 600・中心 320。つままれるは床 560（ぶら下がり）
IDLE_HEIGHT_640 = 480
FLOOR_640 = 600
DEFAULT_FLOORS_640 = {"dragged": 560}


def find_states(src: Path) -> dict[str, Path]:
    """フォルダの PNG をファイル名の状態名で引く（名前の順で最初の 1 枚）。"""
    found = {}
    for p in sorted(src.glob("*.png")):
        low = p.name.lower()
        for s in STATES:
            if s in low and s not in found:
                found[s] = p
    return found


def parse_floors(items: list[str]) -> dict[str, int]:
    out = dict(DEFAULT_FLOORS_640)
    for it in items or []:
        name, _, px = it.partition("=")
        out[name] = int(px)
    return out


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def measured_bbox(im):
    """保存後の RGB 画像で、どれかの色が 250 未満の範囲（build_clips の姿勢合わせと同じ測り方）。"""
    channels = [c.point(lambda v: 255 if v < 250 else 0) for c in im.split()]
    return ImageChops.lighter(ImageChops.lighter(*channels[:2]), channels[2]).getbbox()


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("src", type=Path)
    ap.add_argument("out", type=Path)
    ap.add_argument("--size", type=int, default=640, help="正方形の一辺（16 の倍数。640 か 960）")
    ap.add_argument("--floor", action="append", help="状態=床の y（640 基準）。例 happy=560")
    ap.add_argument("--walk-frame", type=Path, help="描いた歩きのコマを歩行の始点/終点に使う")
    args = ap.parse_args()
    if args.out.exists():
        raise SystemExit(f"出力フォルダが既にある（上書きしない）: {args.out}")
    files = find_states(args.src)
    if "idle" not in files:
        raise SystemExit("idle の静止画が見つからない")
    ratio = args.size / 640
    floors = parse_floors(args.floor)
    hashes = {name: digest(path) for name, path in files.items()}
    images, boxes = {}, {}
    for name, path in files.items():
        with Image.open(path) as src:
            images[name] = src.convert("RGBA")
        boxes[name] = images[name].getchannel("A").point(lambda a: 255 if a > 16 else 0).getbbox()
        if boxes[name] is None:
            raise SystemExit(f"絵が空: {path}")
    heights = {k: b[3] - b[1] for k, b in boxes.items()}
    scale = IDLE_HEIGHT_640 * ratio / heights["idle"]
    if args.walk_frame:
        if "walk" not in files:
            raise SystemExit("--walk-frame には歩行の静止画も要る（高さの基準）")
        with Image.open(args.walk_frame) as w:
            images["walk_frame"] = w.convert("RGBA")
        boxes["walk_frame"] = images["walk_frame"].getchannel("A").point(lambda a: 255 if a > 16 else 0).getbbox()
        heights["walk_frame"] = boxes["walk_frame"][3] - boxes["walk_frame"][1]
        hashes["walk_frame"] = digest(args.walk_frame)
    center = round(args.size / 2)
    results, metadata = {}, {}
    for name, src in images.items():
        s = heights["walk"] * scale / heights[name] if name == "walk_frame" else scale
        left, top, right, bottom = boxes[name]
        size = (round(src.width * s), round(src.height * s))
        resized = src.resize(size, Image.Resampling.LANCZOS)
        sx, sy = size[0] / src.width, size[1] / src.height
        floor = round(floors.get(name, FLOOR_640) * ratio)
        offset = (round(center - (left + right) / 2 * sx), round(floor - bottom * sy))
        canvas = Image.new("RGBA", (args.size, args.size), (255, 255, 255, 255))
        canvas.alpha_composite(resized, dest=offset)
        out_name = "walk" if name == "walk_frame" else name
        results[out_name] = canvas.convert("RGB")
        metadata[out_name] = {"source": str(args.walk_frame if name == "walk_frame" else files[name]), "scale": s,
                              "source_alpha_bbox": boxes[name], "resized_size": size, "offset": offset,
                              "target_bottom_y": floor}
        if out_name in MIRRORED:
            results[out_name + "_right"] = ImageOps.mirror(results[out_name])
            metadata[out_name + "_right"] = {**metadata[out_name], "mirrored_from": out_name + ".png"}
    order = [n for n in ORDER if n in results]
    args.out.mkdir(parents=True)
    records = {}
    for name in order:
        path = args.out / (name + ".png")
        results[name].save(path)
        with Image.open(path) as saved:
            bbox = measured_bbox(saved)
            records[path.name] = {**metadata[name], "size": list(saved.size), "mode": saved.mode, "bbox": bbox,
                                  "bbox_height": bbox[3] - bbox[1], "bottom_y": bbox[3], "center_x": (bbox[0] + bbox[2]) / 2}
    contact = Image.new("RGB", (args.size * len(order), args.size), "white")
    draw = ImageDraw.Draw(contact)
    for i, name in enumerate(order):
        contact.paste(results[name], (i * args.size, 0))
        draw.text((i * args.size + 10, 10), name + ".png", fill="black")
    draw.line((0, round(FLOOR_640 * ratio), contact.width - 1, round(FLOOR_640 * ratio)), fill=(255, 0, 0), width=1)
    contact.save(args.out / "contact.png")

    def floor_ok(n, r):
        f = round(floors.get(n.removesuffix(".png").removesuffix("_right"), FLOOR_640) * ratio)
        return f - 2 <= r["bottom_y"] <= f + 2

    checks = {
        "count": len(records),
        "rgb_square": all(r["size"] == [args.size, args.size] and r["mode"] == "RGB" for r in records.values()),
        "alignment": all(floor_ok(n, r) and center - 2 <= r["center_x"] <= center + 2 for n, r in records.items()),
        "idle_height": records["idle.png"]["bbox_height"],
        "idle_height_expected": round(IDLE_HEIGHT_640 * ratio),
        "inputs_unchanged_sha256": all(digest(Path(metadata[n]["source"])) == hashes.get(n if n != "walk" or not args.walk_frame else "walk_frame", hashes.get(n)) for n in order if "_right" not in n),
    }
    report = {"size": args.size, "floor": round(FLOOR_640 * ratio), "center": center,
              "bbox_convention": "[left, top, right exclusive, bottom exclusive]",
              "measurement": "any RGB channel < 250", "outputs": records, "input_sha256": hashes, "checks": checks}
    (args.out / "verification.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps({"checks": checks, **{n: (r["bbox"], r["bottom_y"], r["center_x"]) for n, r in records.items()}}, indent=1))
    if not (checks["rgb_square"] and checks["alignment"]):
        raise SystemExit("床か中心がそろっていない（上の表を見る）")
    if abs(checks["idle_height"] - checks["idle_height_expected"]) > 2:
        raise SystemExit("待機の高さが基準と合わない")


if __name__ == "__main__":
    main()
