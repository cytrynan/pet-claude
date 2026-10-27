---
type: Asset Source
description: しばちゃん（黒柴）のシートと動画クリップの元素材と作り直し方
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-27T18:40:00+09:00 }
---

# しばちゃんの元素材

4 体目のペット。シート（`spritesheet.webp`）と `pet.json` は `build_pet` で、動画クリップ（`clips/`・`clips.json`）は `scripts/build_clips.py` で作った。作り方はプロジェクトスキル `make-pet-clips`（2026-10-27 K 決定の「960 生成＋等倍」の 2 体目）。元素材はリポジトリに入れていない。

| 素材 | 在処（K の PC） | 中身 |
|---|---|---|
| 状態ごとの静止画 | `C:\Users\meer0\Desktop\黒柴06_状況別イラスト_20261027.zip`（展開 `.steering/20261027-kuroshiba-clips/src/`。git には入れていない） | 待機・歩行・作業中・呼びかけ・喜ぶ・困る・眠る・つままれるの 8 枚（各 1024×1024 透過。zip の時点で頭の幅 約 520px と床 y=900 がそろえてある。喜ぶは 840・つままれるは 820） |

描いた歩きのコマは無い（歩きは動画クリップだけ。シートの歩きの行は静止画 1 枚から `compose` が揺らして作る）。

```bash
CARGO_TARGET_DIR=target-kuroshiba cargo run --release -p pet-core --example build_pet -- <静止画のフォルダ> assets/pets/kuroshiba kuroshiba "しばちゃん" 2
```

待機と睡眠の呼吸は `pet.json` の `breathe`（0.025）でアプリが付ける。呼びかけの言葉（`wakeWord`）は「しばちゃん」。動画のコマは等倍で持つ（`fullCell: true`。2026-10-27 K 決定）。

## 動画クリップ（`clips/` と `clips.json`）

Comfy Cloud の Wan 2.2 14B FLF2V（Lightning 4 ステップ・960×960・16fps）で作った 13 本（2026-10-27。無料枠 3 回: 4 本／8 本＋待機の候補 3 本（1 回目の待機は K 判定「目が怖い」で作り直し。まばたきを外した c を採用）／なでられ 3 候補。1・2 回目は前のアカウントの残り 2 回、3 回目は K が切り替えたアカウントで）。まめドラゴン・りんりんと同じ構成（ループ 4・つなぎ 7・単発 2）。元の動画（白背景・960×960）と姿勢画像はリポジトリに入れていない。

| 素材 | 在処（このリポジトリの `.steering/20261027-kuroshiba-clips/` の下。git には入れていない） |
|---|---|
| 1 回目（止まる・待機・寝る・置かれる） | `run1/kuroshiba_<動き>.mp4` |
| 2 回目（呼びかけ・作業中・喜ぶ・起きる・歩き出す・方向転換・つままれ中・歩き＋待機の候補 a/b/c。採用は c） | `run2/kuroshiba_<動き>.mp4`・`run2/kuroshiba_idle_c.mp4` |
| なでられ（3 候補。採用は {{petted}}） | `run3/kuroshiba_petted_b.mp4` |
| 姿勢画像（足元と大きさをそろえた 960×960・床 900・11 枚） | `poses/`（`verification.json` に絵の範囲。スキルの `align_poses.py --size 960 --floor happy=560` で作る） |
| 作り方のソース・指示文 | `clips-src.json`・`run1.json`・`run2b.json`（8 本＋待機の候補）・`run3.json`（スキルの `make_runs.py` で生成。workflow は `make-comfy-flf2v.py --batch`、Cloud の操作は `run_comfy.py`） |

作り直し: `python -I scripts/build_clips.py .steering/20261027-kuroshiba-clips/clips-src.json assets/pets/kuroshiba`。
