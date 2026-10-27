---
type: Asset Source
description: ごまちゃん（ハムスター灰）のシートと動画クリップの元素材と作り直し方
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-28T02:20:00+09:00 }
---

# ごまちゃんの元素材

6 体目のペット。シート（`spritesheet.webp`）と `pet.json` は `build_pet` で、動画クリップ（`clips/`・`clips.json`）は `scripts/build_clips.py` で作った。作り方はプロジェクトスキル `make-pet-clips`（2026-10-27 K 決定の「960 生成＋等倍」）。元素材はリポジトリに入れていない。

| 素材 | 在処（K の PC） | 中身 |
|---|---|---|
| 状態ごとの静止画 | `C:\Users\meer0\Desktop\ハムスター09_状況別イラスト_20261027.zip`（展開 `.steering/20261028-goma-clips/src/`。git には入れていない。中の `package_hamster_states.py` は実行していない） | 待機・歩行・作業中・呼びかけ・喜ぶ・困る・眠る・つままれるの 8 枚（各 1024×1024 透過。接地 6 枚は床 y=900、喜ぶ 840、つままれる 820） |
| 寝姿を直した 8 枚 | `.steering/20261028-goma-clips/srcfix-0.9/`（git には入れていない） | `07-sleeping.png` だけ、床の中心を軸に 0.9 倍に縮めたもの（色はそのまま。灰色の毛の明るさは 8 枚で 217〜224 にそろっていた）。0.9 倍はこむぎちゃんで K が選んだ大きさに合わせた。道具は `.steering/20261028-goma-clips/fix_sleep.py`（`python -I fix_sleep.py src srcfix-0.9 0.9 nocolor`） |

シートも動画も、直した 8 枚（`srcfix-0.9/`）から作った。

```bash
cargo run --release -p pet-core --example build_pet -- .steering/20261028-goma-clips/srcfix-0.9 assets/pets/goma goma "ごまちゃん" 2
```

待機と睡眠の呼吸は `pet.json` の `breathe`（0.025）でアプリが付ける。呼びかけの言葉（`wakeWord`）は「ごまちゃん」。動画のコマは等倍で持つ（`fullCell: true`。2026-10-27 K 決定）。

## 動画クリップ（`clips/` と `clips.json`）

Comfy Cloud の Wan 2.2 14B FLF2V（Lightning 4 ステップ・960×960・16fps）で作った 13 本（2026-10-28。無料枠 3 回: 4 本／8 本／なでられ 3 候補）。構成はまめドラゴンと同じ（ループ 4・つなぎ 7・単発 2）。元の動画（白背景・960×960）と姿勢画像はリポジトリに入れていない。

| 素材 | 在処（このリポジトリの `.steering/20261028-goma-clips/` の下。git には入れていない） |
|---|---|
| 1 回目（止まる・待機・寝る・置かれる） | `run1/goma_<動き>.mp4` |
| 2 回目（呼びかけ・作業中・喜ぶ・起きる・歩き出す・方向転換・つままれ中・歩き） | `run2/goma_<動き>.mp4` |
| なでられ（3 候補。採用は b） | `run3/goma_petted_b.mp4` |
| 姿勢画像（足元と大きさをそろえた 960×960・床 900・11 枚。寝姿は 0.9 倍） | `poses/`（`verification.json` に絵の範囲） |
| 作り方のソース・指示文 | `clips-src.json`・`run1.json`〜`run3.json`（スキル `make-pet-clips` の `make_runs.py` で生成。寝る・起きるに「色を保つ」の一文、3 本とも否定語に darker colors などを手で足した） |

作り直し: `python -I scripts/build_clips.py .steering/20261028-goma-clips/clips-src.json assets/pets/goma`。
