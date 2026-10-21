---
type: Asset Source
description: まめドラゴンのシートの元素材と作り直し方
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-21T17:20:00+09:00 }
---

# まめドラゴンの元素材

シート（`spritesheet.webp`）と `pet.json` は `build_pet` で作ったもの。元素材はリポジトリに入れていない。

| 素材 | 在処（K の PC） | 中身 |
|---|---|---|
| 状態ごとの静止画 | `C:\Users\meer0\Downloads\まめドラゴン.zip` | 待機・歩く・作業中・呼びかけ・喜ぶ・困る・眠る・つままれるの 8 枚（各 1312×1199、つままれだけ 1199×1312） |
| 歩きの 8 コマ | `C:\Users\meer0\Documents\Codex\2026-10-21\claudecode-gpt\outputs\mame-dragon-walk-01.zip` の `frames/` | 右向きに歩く 8 コマ（各 448×448）。元は 1 秒で 1 周 |

2 つの zip を別々の空のフォルダに展開してから、リポジトリのルートで次を実行する。

```bash
cargo run --release -p pet-core --example build_pet -- <静止画のフォルダ> assets/pets/mame-dragon mame-dragon "まめドラゴン" 2 --frames walk=<歩きの frames フォルダ>
```

待機と睡眠の呼吸はシートに入れず、`pet.json` の `breathe`（0.025）でアプリが付ける。
