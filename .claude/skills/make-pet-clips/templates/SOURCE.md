---
type: Asset Source
description: {{表示名}}のシートと動画クリップの元素材と作り直し方
generated: { by: kuroko/<モデルID>, at: {{日時}} }
---

# {{表示名}}の元素材

シート（`spritesheet.webp`）と `pet.json` は `build_pet` で、動画クリップ（`clips/`・`clips.json`）は `scripts/build_clips.py` で作った。元素材はリポジトリに入れていない。

| 素材 | 在処（K の PC） | 中身 |
|---|---|---|
| 状態ごとの静止画 | `{{zip のパス}}`（展開 `{{展開先}}`） | 待機・歩行・作業中・呼びかけ・喜ぶ・困る・眠る・つままれるの 8 枚（{{大きさ・床の情報}}） |

```bash
CARGO_TARGET_DIR={{target}} cargo run --release -p pet-core --example build_pet -- <静止画のフォルダ> assets/pets/{{id}} {{id}} "{{表示名}}" 2
```

待機と睡眠の呼吸は `pet.json` の `breathe`（0.025）でアプリが付ける。呼びかけの言葉（`wakeWord`）は「{{呼びかけ}}」。動画のコマは等倍で持つ（`fullCell: true`。2026-10-27 K 決定）。

## 動画クリップ（`clips/` と `clips.json`）

Comfy Cloud の Wan 2.2 14B FLF2V（Lightning 4 ステップ・{{960}}×{{960}}・16fps）で作った 13 本（{{日付}}。無料枠 {{n}} 回）。構成はまめドラゴンと同じ（ループ 4・つなぎ 7・単発 2）。元の動画と姿勢画像はリポジトリに入れていない。

| 素材 | 在処（このリポジトリの `.steering/{{日付}}-{{id}}-clips/` の下。git には入れていない） |
|---|---|
| 1 回目（止まる・待機・寝る・置かれる） | `run1/{{id}}_<動き>.mp4` |
| 2 回目（呼びかけ・作業中・喜ぶ・起きる・歩き出す・方向転換・つままれ中・歩き） | `run2/{{id}}_<動き>.mp4` |
| なでられ（3 候補。採用は {{a}}） | `run3/{{id}}_petted_{{a}}.mp4` |
| 姿勢画像（足元と大きさをそろえた {{960}}×{{960}}） | `poses/`（`verification.json` に絵の範囲） |
| 作り方のソース・指示文 | `clips-src.json`・`run1.json`〜`run3.json`（スキル `make-pet-clips`） |

作り直し: `python -I scripts/build_clips.py .steering/{{日付}}-{{id}}-clips/clips-src.json assets/pets/{{id}}`。
