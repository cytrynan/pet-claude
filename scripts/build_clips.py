"""動画からペットのクリップ（透過 WebP のコマと clips.json）を作る。

    python scripts/build_clips.py <ソースの JSON> <ペットのフォルダ>

ソースの JSON（例: `assets/pets/mame-dragon/clips-src.json`）:
    {"poses": "<姿勢画像のフォルダ（verification.json がある）>",
     "canvas": 640,   # 動画と姿勢画像の一辺（省略時 640。床は 600/640・中心は 1/2 の位置）
     "src_fps": 16, "fps": 8, "drop_tail": 3,
     "cell": [384, 416], "floor": 404, "ref_height": 354, "ref_pose": "idle.png",
     "clips": [{"name", "kind", "from": [...], "to": [...], "video", "start_pose",
                "canvas"?, "poses"?}]}   # 本ごとに別の大きさ・別の姿勢画像のフォルダで作った動画を混ぜられる

なぜこの形か（.steering/20261021-clip-playback/design.md §2）:
- Wan 2.2 FLF2V の動画は最後の 2〜3 コマで彩度が落ちてかすむ（2026-10-21 実測 24→18）ので、末尾を捨てる。
- 背景は白で作った（透過のまま渡すと透過部が不定色になる）ので、ここで白を抜く。
- 動画ごとに足元と大きさがずれると、つなぎで体が跳ねる。1 コマ目を始点の姿勢画像に合わせ、
  全クリップを共通の倍率でシートのコマ（384×416・床 y=404）へ置く。
"""

import json
import subprocess
import sys
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage

# どの色も これ以上なら白背景とみなす（動画の圧縮で背景は 248〜253 に揺れる。キャラの最も明るい
# クリーム色は端から閉じた輪郭の内側にあるので、端からつながった画素だけを背景にすれば食わない）
BG_MIN = 236
# 縁の画素のアルファを白との混ざり具合から戻すときの「完全に不透明」とみなす明るさ（輪郭の茶色の最小値付近）
EDGE_DARK = 70
# キャンバス（動画）の既定の大きさ。床と中心はこの大きさでの 600・320 を比例で伸ばす
# （2026-10-27 画質の試しで 960×960 の動画も混ぜられるようにした）
SRC = 640


def floor_center(canvas: int) -> tuple[float, float]:
    """キャンバス座標の床の y と横の中心（640 なら 600・320）。"""
    return canvas * 600 / SRC, canvas / 2


def read_frames(video: Path, canvas: int = SRC) -> np.ndarray:
    """ffmpeg で全コマを (n, canvas, canvas, 3) の uint8 にする。"""
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", str(video), "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
        capture_output=True, check=True,
    ).stdout
    n = len(raw) // (canvas * canvas * 3)
    return np.frombuffer(raw, np.uint8)[: n * canvas * canvas * 3].reshape(n, canvas, canvas, 3)


def pick(n: int, src_fps: int, fps: int, drop_tail: int) -> list[int]:
    """使うコマの番号。末尾 drop_tail コマを捨て、src_fps → fps に間引く。"""
    step = src_fps // fps
    return list(range(0, max(n - drop_tail, 1), step))


def remove_white(rgb: np.ndarray) -> np.ndarray:
    """白背景を抜いた RGBA を返す。端からつながった明るい画素を背景にし、縁はアルファを戻す。"""
    light = rgb.min(axis=2) >= BG_MIN
    labels, _ = ndimage.label(light)
    border = np.unique(np.concatenate([labels[0], labels[-1], labels[:, 0], labels[:, -1]]))
    bg = np.isin(labels, border[border > 0])
    # 背景の中に残った小さなかけら（圧縮のノイズ。2026-10-21 隅の 1 画素が 235 で残り、絵の範囲を
    # キャンバス全体に広げた）は背景にする。いちばん大きいかたまりの 1% 未満を捨てる
    fg_labels, n = ndimage.label(~bg)
    if n > 1:
        sizes = ndimage.sum(np.ones_like(fg_labels), fg_labels, index=range(1, n + 1))
        small = [i + 1 for i, s in enumerate(sizes) if s < 0.01 * sizes.max()]
        bg |= np.isin(fg_labels, small)
    alpha = np.where(bg, 0.0, 1.0)
    # 背景に接する 2px の帯は、白と混ざった画素として a = (255 - 最小値) / (255 - EDGE_DARK) で戻す
    edge = ndimage.binary_dilation(bg, iterations=2) & ~bg
    m = rgb.min(axis=2).astype(np.float32)
    a_edge = np.clip((255.0 - m) / (255.0 - EDGE_DARK), 0.0, 1.0)
    alpha = np.where(edge, a_edge, alpha)
    out = np.zeros(rgb.shape[:2] + (4,), np.float32)
    a3 = alpha[..., None]
    # 色は白との混ざりを外す: C = (観測 - (1 - a) × 255) / a
    color = (rgb.astype(np.float32) - (1.0 - a3) * 255.0) / np.maximum(a3, 1e-3)
    out[..., :3] = np.clip(np.where(a3 > 0, color, 0.0), 0, 255)
    out[..., 3] = alpha * 255.0
    return out.round().astype(np.uint8)


def bbox(rgba: np.ndarray, thresh: int = 16) -> tuple[int, int, int, int] | None:
    """アルファが thresh を超える範囲 (left, top, right, bottom)。right・bottom は含まない。"""
    ys, xs = np.nonzero(rgba[..., 3] > thresh)
    if len(xs) == 0:
        return None
    return int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1


def align(first_box, pose_box) -> tuple[float, float, float]:
    """1 コマ目の絵を始点の姿勢画像に重ねる変換 (倍率, x の足し分, y の足し分)。
    倍率は高さの比、足元（下端）と横の中心を合わせる。キャンバス座標 → キャンバス座標。"""
    l, t, r, b = first_box
    pl, pt, pr, pb = pose_box
    s = (pb - pt) / (b - t)
    dx = (pl + pr) / 2 - s * (l + r) / 2
    dy = pb - s * b
    return s, dx, dy


def place(rgba: np.ndarray, s: float, dx: float, dy: float, k: float, cell, floor: int, canvas: int = SRC) -> Image.Image:
    """キャンバスの絵を、(s, dx, dy) で姿勢に合わせ、共通倍率 k でコマへ置く。
    コマ座標: x' = cell_w/2 + (X - cx) k、y' = floor + (Y - fy) k（X, Y は姿勢合わせ後。640 なら床 fy=600・中心 cx=320）。
    k はキャンバス 640 基準の倍率（960 の動画は 640 相当に縮めてから同じ k を掛ける＝キャラの大きさが同じになる）。"""
    fy, cx = floor_center(canvas)
    k = k * SRC / canvas
    a = s * k
    ox = cell[0] / 2 + (dx - cx) * k
    oy = floor + (dy - fy) * k
    img = Image.fromarray(rgba, "RGBA")
    # 事前乗算してから縮める（縁が黒ずまないように）。アフィン変換の逆写像を渡す
    pre = img.convert("RGBa")
    out = pre.transform(cell, Image.AFFINE, (1 / a, 0, -ox / a, 0, 1 / a, -oy / a), resample=Image.BICUBIC)
    return out.convert("RGBA")


def placed_box(box, s, dx, dy):
    """キャンバス上の範囲を姿勢合わせ後の座標へ。"""
    l, t, r, b = box
    return s * l + dx, s * t + dy, s * r + dx, s * b + dy


def to_640(box, canvas: int):
    """別の大きさのキャンバスの範囲を 640 基準の座標へ（倍率の計算を 640 でそろえるため）。"""
    f = SRC / canvas
    return tuple(v * f for v in box)


def common_scale(boxes, cell, floor, ref_k) -> float:
    """全コマ（姿勢合わせ後・640 基準の座標）がコマに 98% で収まり、かつ待機の高さが ref に合う倍率の小さい方。"""
    half = cell[0] / 2
    k = ref_k
    for l, t, r, b in boxes:
        k = min(k, 0.98 * half / max(320 - l, r - 320), 0.98 * floor / (600 - t))
        if b > 600:
            k = min(k, 0.98 * (cell[1] - floor) / (b - 600))
    return k


def main(src_json: Path, pet_dir: Path) -> None:
    spec = json.loads(src_json.read_text(encoding="utf-8"))
    base = src_json.parent
    pose_sets = {}

    def poses_of(rel: str) -> dict:
        if rel not in pose_sets:
            pose_sets[rel] = json.loads((base / rel / "verification.json").read_text(encoding="utf-8"))["outputs"]
        return pose_sets[rel]

    cell = tuple(spec["cell"])
    floor = spec["floor"]

    loaded = []
    for c in spec["clips"]:
        canvas = c.get("canvas", spec.get("canvas", SRC))
        poses = poses_of(c.get("poses", spec["poses"]))
        frames = read_frames(base / c["video"], canvas)
        # 末尾のかすみを捨てると、つなぎが終点の姿勢まで届かず次のループとの継ぎ目で跳ねることがある
        # （2026-10-21 walk_start→walk で頭が 24px）。そのクリップだけ drop_tail を上書きできる
        idx = pick(len(frames), spec["src_fps"], spec["fps"], c.get("drop_tail", spec["drop_tail"]))
        rgba = [remove_white(frames[i]) for i in idx]
        pose = poses[c["start_pose"]]["bbox"]
        s, dx, dy = align(bbox(rgba[0]), pose)
        boxes = [to_640(placed_box(bbox(f), s, dx, dy), canvas) for f in rgba if bbox(f)]
        loaded.append((c, rgba, (s, dx, dy), boxes, canvas))
        print(f"{c['name']}: {len(rgba)} コマ  姿勢合わせ 倍率 {s:.3f}" + (f"  キャンバス {canvas}" if canvas != SRC else ""))

    ref = to_640(poses_of(spec["poses"])[spec["ref_pose"]]["bbox"], spec.get("canvas", SRC))
    ref_k = spec["ref_height"] / (ref[3] - ref[1])
    k_fit = common_scale([b for *_, boxes, _ in loaded for b in boxes], cell, floor, ref_k)
    # spec の scale で決め打ちできる。全部を収める倍率は、横に長い姿勢（眠る）1 つに引っ張られて
    # 全体が小さくなる（まめドラゴンは 0.588＝シートの待機の 8 割）ので、少しの見切れを許して大きくする
    k = spec.get("scale", k_fit)
    print(f"共通の倍率 {k:.4f}（全部が収まる倍率 {k_fit:.4f}・待機の高さの基準 {ref_k:.4f}）")
    for c, _, _, boxes, _ in loaded:
        over = max(max((320 - l) * k - cell[0] / 2, (r - 320) * k - cell[0] / 2, (600 - t) * k - floor) for l, t, r, _ in boxes)
        if over > 0:
            print(f"  見切れ {c['name']}: 最大 {over:.1f}px（コマの端から）")

    out_clips = []
    for c, rgba, (s, dx, dy), _, canvas in loaded:
        d = pet_dir / "clips" / c["name"]
        d.mkdir(parents=True, exist_ok=True)
        for old in d.glob("*.webp"):
            old.unlink()
        for i, f in enumerate(rgba):
            place(f, s, dx, dy, k, cell, floor, canvas).save(d / f"{i:03d}.webp", lossless=False, quality=92, alpha_quality=100, method=6)
        out_clips.append({"name": c["name"], "kind": c["kind"], "from": c["from"], "to": c["to"], "frames": len(rgba)})
    (pet_dir / "clips.json").write_text(
        json.dumps({"fps": spec["fps"], "clips": out_clips}, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8", newline="\n")  # Windows で CRLF にしない（リポジトリは LF）
    print(f"ok {len(out_clips)} 本 → {pet_dir / 'clips.json'}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(Path(sys.argv[1]), Path(sys.argv[2]))
