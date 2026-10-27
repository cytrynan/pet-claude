---
type: Asset Source
description: りんりん（パンダ）のシートと動画クリップの元素材と作り直し方
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-27T13:30:00+09:00 }
---

# りんりんの元素材

2 体目のペット。シート（`spritesheet.webp`）と `pet.json` は `build_pet` で、動画クリップ（`clips/`・`clips.json`）は `scripts/build_clips.py` で作った。元素材はリポジトリに入れていない。

| 素材 | 在処（K の PC） | 中身 |
|---|---|---|
| 状態ごとの静止画 | `C:\Users\meer0\Desktop\パンダ03_状況別イラスト_20261027.zip`（展開 `C:\Users\meer0\Desktop\rinrin-src\`） | 待機・歩行・作業中・呼びかけ・喜ぶ・困る・眠る・つままれるの 8 枚（各 1024×1024 透過。zip の時点で頭の幅 約 560px と床 y=900 がそろえてある。喜ぶは 840・つままれるは 820） |

まめドラゴンと違い、描いた歩きのコマは無い（歩きは動画クリップだけ。シートの歩きの行は静止画 1 枚から `compose` が揺らして作る）。

```bash
CARGO_TARGET_DIR=target-rinrin cargo run --release -p pet-core --example build_pet -- <静止画のフォルダ> assets/pets/rinrin rinrin "りんりん" 2
```

待機と睡眠の呼吸は `pet.json` の `breathe`（0.025）でアプリが付ける。呼びかけの言葉（`wakeWord`）は「りんちゃん」（K の呼び方）。

## 動画クリップ（`clips/` と `clips.json`）

Comfy Cloud の Wan 2.2 14B FLF2V（Lightning 4 ステップ・640×640・16fps）で作った 13 本（2026-10-27。無料枠 3 回: 4 本＋8 本＋なでられ 3 候補）。まめドラゴンと同じ構成（ループ 4・つなぎ 7・単発 2）。元の動画（白背景・640×640）と姿勢画像はリポジトリに入れていない。

| 素材 | 在処（このリポジトリの `.steering/20261027-rinrin-clips/` の下。git には入れていない） |
|---|---|
| 1 回目（止まる・待機・寝る・置かれる） | `run1/rinrin_<動き>.mp4` |
| 2 回目（呼びかけ・作業中・喜ぶ・起きる・歩き出す・方向転換・つままれ中・歩き） | `run2/rinrin_<動き>.mp4` |
| なでられ（3 候補。採用は a） | `run3/rinrin_petted_a.mp4` |
| 姿勢画像（足元と大きさをそろえた 640×640・11 枚） | `poses/`（`verification.json` に絵の範囲。`align_poses.py` で作る） |
| 作り方のソース・指示文 | `clips-src.json`・`run1.json`〜`run3.json`（workflow は `make-comfy-flf2v.py --batch`、Cloud の操作は `run_comfy.py`） |

作り直し: `python -I scripts/build_clips.py .steering/20261027-rinrin-clips/clips-src.json assets/pets/rinrin`。
