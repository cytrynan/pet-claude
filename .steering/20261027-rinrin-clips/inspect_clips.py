"""生成した動画の検分: 始点一致・終点のずれ・最後のコマの彩度落ち・手の混入（肌色）を表にし、
contact.png（各本 5 コマ）と preview-Nup.mp4（並べて再生）を作る。

    python -I inspect_clips.py <run の JSON> <run のフォルダ>

まめドラゴンの検分（`../20261021-wan-transitions/tasklist.md`「1 回目の検分」）と同じ物差し:
- 始点/終点の一致 = 動画の 1 コマ目/最後のコマと姿勢画像（640×640 白背景）の RGB の平均絶対差
- 彩度落ち = 最後の数コマの HSV の S の平均（かすむと下がる）
- 手の混入 = 上端 1/3 で「肌色」（R>180, 120<G<200, 100<B<180, R>G>B）の画素数。
  まめドラゴンで手が入った回は 12574、無い回は 27〜42
"""

import json
import subprocess
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
SRC = 640


def read_frames(video: Path) -> np.ndarray:
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", str(video), "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
                         capture_output=True, check=True).stdout
    n = len(raw) // (SRC * SRC * 3)
    return np.frombuffer(raw, np.uint8)[: n * SRC * SRC * 3].reshape(n, SRC, SRC, 3)


def mean_abs_diff(a: np.ndarray, b: np.ndarray) -> float:
    return float(np.abs(a.astype(np.int16) - b.astype(np.int16)).mean())


def saturation(frame: np.ndarray) -> float:
    hsv = np.array(Image.fromarray(frame).convert("HSV"))
    mask = frame.min(axis=2) < 236  # 背景（白）を除く
    return float(hsv[..., 1][mask].mean()) if mask.any() else 0.0


def skin_pixels(frame: np.ndarray) -> int:
    top = frame[: SRC // 3].astype(np.int16)
    r, g, b = top[..., 0], top[..., 1], top[..., 2]
    return int(((r > 180) & (g > 120) & (g < 200) & (b > 100) & (b < 180) & (r > g) & (g > b)).sum())


def main(spec_path: Path, run_dir: Path) -> None:
    spec = json.loads(spec_path.read_text(encoding="utf-8"))
    poses = HERE / "poses"
    rows, strips = [], []
    for c in spec["clips"]:
        video = run_dir / (c["name"] + ".mp4")
        frames = read_frames(video)
        start = np.array(Image.open(poses / c["start"]).convert("RGB"))
        end = np.array(Image.open(poses / c.get("end", c["start"])).convert("RGB"))
        sats = [saturation(f) for f in frames]
        row = {
            "name": c["name"], "frames": len(frames), "length": c["length"],
            "start_diff": round(mean_abs_diff(frames[0], start), 2),
            "end_diff": round(mean_abs_diff(frames[-1], end), 2),
            "end_diff_minus3": round(mean_abs_diff(frames[-4], end), 2),
            "sat_mid": round(float(np.mean(sats[len(sats) // 4: 3 * len(sats) // 4])), 1),
            "sat_last3": [round(s, 1) for s in sats[-3:]],
            "skin_max": max(skin_pixels(f) for f in frames),
        }
        rows.append(row)
        picks = [0, len(frames) // 4, len(frames) // 2, 3 * len(frames) // 4, len(frames) - 1]
        strip = Image.new("RGB", (320 * 5, 320), "white")
        for i, k in enumerate(picks):
            strip.paste(Image.fromarray(frames[k]).resize((320, 320)), (i * 320, 0))
        d = ImageDraw.Draw(strip)
        d.text((6, 4), f"{c['name']}  frames {picks}", fill="red")
        strips.append(strip)
    contact = Image.new("RGB", (320 * 5, 320 * len(strips)), "white")
    for i, s in enumerate(strips):
        contact.paste(s, (0, i * 320))
    contact.save(run_dir / "contact.png")
    (run_dir / "inspection.json").write_text(json.dumps(rows, indent=1, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")
    print("| 本 | コマ | 始点差 | 終点差 | 終点差(末尾-3) | 彩度(中) | 彩度(最後3) | 肌色max |")
    print("|---|---|---|---|---|---|---|---|")
    for r in rows:
        print(f"| {r['name']} | {r['frames']}/{r['length']} | {r['start_diff']} | {r['end_diff']} | {r['end_diff_minus3']} | {r['sat_mid']} | {r['sat_last3']} | {r['skin_max']} |")
    # 並べて再生する preview（各 320px・横に並べ、短い本は最後のコマで止める）
    n = len(rows)
    inputs = []
    for c in spec["clips"]:
        inputs += ["-i", str(run_dir / (c["name"] + ".mp4"))]
    longest = max(r["frames"] for r in rows)
    fc = "".join(f"[{i}:v]scale=320:320,tpad=stop_mode=clone:stop_duration={longest / 16 + 1}[v{i}];" for i in range(n))
    fc += "".join(f"[v{i}]" for i in range(n)) + f"hstack=inputs={n}[out]"
    out = run_dir / f"preview-{n}up.mp4"
    subprocess.run(["ffmpeg", "-y", "-v", "error", *inputs, "-filter_complex", fc, "-map", "[out]", "-t", str(longest / 16 + 1),
                    "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "20", str(out)], check=True)
    print(f"ok {out} / {run_dir / 'contact.png'}")


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]))
