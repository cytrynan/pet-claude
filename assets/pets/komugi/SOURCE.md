---
type: Asset Source
description: こむぎちゃん（ハムスター茶）のシートと動画クリップの元素材と作り直し方
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-28T01:50:00+09:00 }
---

# こむぎちゃんの元素材

5 体目のペット。シート（`spritesheet.webp`）と `pet.json` は `build_pet` で、動画クリップ（`clips/`・`clips.json`）は `scripts/build_clips.py` で作った。作り方はプロジェクトスキル `make-pet-clips`（2026-10-27 K 決定の「960 生成＋等倍」）。元素材はリポジトリに入れていない。

| 素材 | 在処（K の PC） | 中身 |
|---|---|---|
| 状態ごとの静止画 | `C:\Users\meer0\Desktop\ハムスター02_状況別イラスト_20261027.zip`（展開 `.steering/20261028-komugi-clips/src/`。git には入れていない。中の `package_hamster_states.py` は実行していない） | 待機・歩行・作業中・呼びかけ・喜ぶ・困る・眠る・つままれるの 8 枚（各 1024×1024 透過。接地 6 枚は床 y=900、喜ぶ 840、つままれる 820） |
| 寝姿を直した 8 枚 | `.steering/20261028-komugi-clips/srcfix-0.9/`（git には入れていない） | 上の 8 枚のうち `07-sleeping.png` だけ、毛の彩度を立ち姿の中央値にそろえ（毛の彩度の中央値 110 → 100・HSV の S を 0.914 倍）、床の中心を軸に 0.9 倍に縮めたもの。ほかの 7 枚は同じ。K 指摘「寝るやつだけ色のこさと大きさが異なる」（2026-10-28）・0.9 倍は K の選択。作った道具は `.steering/20261028-komugi-clips/fix_sleep.py`（`python -I fix_sleep.py src srcfix-0.9 0.9`） |

シートも動画も、直した 8 枚（`srcfix-0.9/`）から作った。

```bash
cargo run --release -p pet-core --example build_pet -- .steering/20261028-komugi-clips/srcfix-0.9 assets/pets/komugi komugi "こむぎちゃん" 2
```

待機と睡眠の呼吸は `pet.json` の `breathe`（0.025）でアプリが付ける。呼びかけの言葉（`wakeWord`）は「こむぎちゃん」。動画のコマは等倍で持つ（`fullCell: true`。2026-10-27 K 決定）。

## 動画クリップ（`clips/` と `clips.json`）

Comfy Cloud の Wan 2.2 14B FLF2V（Lightning 4 ステップ・960×960・16fps）で作った 13 本（2026-10-28。無料枠 3 回: 4 本／8 本＋寝るの撮り直し 2 候補／なでられ 3 候補）。構成はまめドラゴンと同じ（ループ 4・つなぎ 7・単発 2）。元の動画（白背景・960×960）と姿勢画像はリポジトリに入れていない。

| 素材 | 在処（このリポジトリの `.steering/20261028-komugi-clips/` の下。git には入れていない） |
|---|---|
| 1 回目（止まる・待機・寝る・置かれる。寝るは不採用） | `run1/komugi_<動き>.mp4` |
| 2 回目（呼びかけ・作業中・喜ぶ・起きる・歩き出す・方向転換・つままれ中・歩き＋寝るの撮り直し a/b。寝るは b を採用） | `run2/komugi_<動き>.mp4`・`run2/komugi_sleep_b.mp4` |
| なでられ（3 候補。採用は a） | `run3/komugi_petted_a.mp4` |
| 姿勢画像（足元と大きさをそろえた 960×960・床 900・11 枚。寝姿は直した版） | `poses/`（`verification.json` に絵の範囲。直す前は `poses-v1/`） |
| 作り方のソース・指示文 | `clips-src.json`・`run1.json`〜`run3.json`（スキル `make-pet-clips` の `make_runs.py` で生成。run2 は寝るの撮り直し 2 本と「色を明るいまま保つ」の一文・否定語を手で足した） |

作り直し: `python -I scripts/build_clips.py .steering/20261028-komugi-clips/clips-src.json assets/pets/komugi`。
