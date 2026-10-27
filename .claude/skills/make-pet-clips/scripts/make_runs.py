"""13 本の指示文のひな形（templates/prompts.json）にキャラの説明を差し込み、Comfy の実行 3 回分の JSON
（run1.json＝試し 4 本・run2.json＝8 本・run3.json＝なでられ 3 候補）と clips-src.json を書く。

    python -I make_runs.py --pet rinrin --out <作業フォルダ> --size 960 \
        --subject "cute chibi baby panda" --noun panda \
        --design "thick dark brown outline, flat white fur, dark charcoal ears, eye patches, arms and legs, pink cheeks" \
        --idle " with its paws together in front of its chest" --wiggle "its round ears twitch slightly" --hand paw \
        --avoid "green, dragon"

- --avoid は前のペットの特徴が混ざらないようにネガティブへ足す語（りんりんでは green, dragon）。
- 出来た JSON は 2dmodel の make-comfy-flf2v.py --batch に渡す（SKILL.md の手順 4）。
"""

import argparse
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
TEMPLATE = HERE.parent / "templates" / "prompts.json"


def render(template: dict, pet: str, size: int, fields: dict, avoid: str) -> dict[str, dict]:
    """ひな形から run1〜run3 の JSON（dict）を作る。戻り値は {"run1": {...}, ...}。"""
    tail = template["tail"].format(**fields)
    negative = template["negative"] + (", " + avoid if avoid else "")
    runs = {}
    for run, names in template["runs"].items():
        clips = []
        for n in names:
            c = template["clips"][n]
            prompt = c["prompt"].format(tail=tail, **fields)
            if "{" in prompt or "}" in prompt:
                raise ValueError(f"{n}: 差し込みが残っている: {prompt}")
            if (c["length"] - 1) % 4:
                raise ValueError(f"{n}: length は 4n+1")
            clip = {"name": f"{pet}_{n}", "start": c["start"], "length": c["length"], "prompt": prompt}
            if "end" in c:
                clip["end"] = c["end"]
            if "seed" in c:
                clip["seed"] = c["seed"]
            clips.append(clip)
        neg = negative + (template["negative_petted"] if run == "run3" else "")
        runs[run] = {"name": f"{pet}_{run}", "size": [size, size], "negative_extra": neg, "clips": clips}
    return runs


def clips_src(pet: str, size: int, poses: str, petted: str = "petted_a") -> dict:
    """scripts/build_clips.py に渡すソース（13 本の from/to と動画の在処）。"""
    r1 = {"idle", "stop", "sleep", "put_down"}

    def video(n):
        run = "run1" if n in r1 else ("run3" if n.startswith("petted") else "run2")
        return f"{run}/{pet}_{n}.mp4"

    defs = [
        ("idle", "loop", ["idle", "left"], ["idle", "left"], "idle.png", "idle"),
        ("walk", "loop", ["walk", "right"], ["walk", "right"], "walk.png", "walk"),
        ("working", "loop", ["working", "left"], ["working", "left"], "working.png", "working"),
        ("grabbed", "loop", ["dragged", "front"], ["dragged", "front"], "dragged.png", "grabbed"),
        ("stop", "transition", ["walk", "right"], ["idle", "right"], "walk.png", "stop"),
        ("walk_start", "transition", ["idle", "right"], ["walk", "right"], "idle_right.png", "walk_start"),
        ("turn", "transition", ["idle", "right"], ["idle", "left"], "idle_right.png", "turn"),
        ("sleep", "transition", ["idle", "left"], ["sleep", "left"], "idle.png", "sleep"),
        ("wake", "transition", ["sleep", "left"], ["idle", "left"], "sleeping.png", "wake"),
        ("put_down", "transition", ["dragged", "front"], ["idle", "left"], "dragged.png", "put_down"),
        ("call", "transition", ["idle", "left"], ["attention", "front"], "idle.png", "call"),
        ("happy", "once", ["idle", "left"], ["idle", "left"], "idle.png", "happy"),
        ("petted", "once", ["idle", "left"], ["idle", "left"], "idle.png", petted),
    ]
    clips = []
    for name, kind, frm, to, pose, src in defs:
        c = {"name": name, "kind": kind, "from": frm, "to": to, "video": video(src), "start_pose": pose}
        if name == "walk_start":
            c["drop_tail"] = 0  # 終点まで流し切らないと次の歩きのループで跳ねる（2026-10-21 頭が 24px）
        clips.append(c)
    return {"poses": poses, "canvas": size, "src_fps": 16, "fps": 8, "drop_tail": 3, "cell": [384, 416],
            "floor": 404, "ref_height": 354, "ref_pose": "idle.png", "clips": clips}


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--pet", required=True, help="ペット ID（出力名の接頭辞）")
    ap.add_argument("--out", type=Path, required=True, help="run1〜3.json と clips-src.json を書くフォルダ")
    ap.add_argument("--size", type=int, default=960, help="動画の一辺（2026-10-27 K 決定で既定 960）")
    ap.add_argument("--subject", required=True, help="例: cute chibi baby panda")
    ap.add_argument("--noun", required=True, help="例: panda")
    ap.add_argument("--design", required=True, help="見た目の列挙（輪郭・色・部位）")
    ap.add_argument("--idle", default="", help="立ち姿の補足。先頭に空白。例 ' with its paws together in front of its chest'")
    ap.add_argument("--wiggle", required=True, help="待機の小さな動き。例 its round ears twitch slightly")
    ap.add_argument("--hand", default="hand", help="paw か hand")
    ap.add_argument("--avoid", default="", help="ネガティブに足す語（前のペットの特徴）")
    ap.add_argument("--poses", default="poses", help="clips-src.json に書く姿勢画像のフォルダ（作業フォルダからの相対）")
    args = ap.parse_args()
    template = json.loads(TEMPLATE.read_text(encoding="utf-8"))
    fields = {"subject": args.subject, "noun": args.noun, "design": args.design, "idle": args.idle,
              "wiggle": args.wiggle, "hand": args.hand}
    runs = render(template, args.pet, args.size, fields, args.avoid)
    args.out.mkdir(parents=True, exist_ok=True)
    for run, spec in runs.items():
        (args.out / f"{run}.json").write_text(json.dumps(spec, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
    (args.out / "clips-src.json").write_text(json.dumps(clips_src(args.pet, args.size, args.poses), ensure_ascii=False, indent=2) + "\n",
                                             encoding="utf-8", newline="\n")
    print(f"ok {args.out}: run1.json run2.json run3.json clips-src.json（{sum(len(r['clips']) for r in runs.values())} 本・{args.size}×{args.size}）")


if __name__ == "__main__":
    main()
