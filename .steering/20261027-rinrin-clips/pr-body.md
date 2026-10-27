## なぜ

K 指示（2026-10-27）: 2 体目のペット「りんりん」（パンダ）を、まめドラゴンと同じ手順で作る。状態別の静止画 8 枚（K が用意した zip）からシートを、Comfy Cloud の Wan 2.2 14B FLF2V で動画クリップ 13 本を作り、設定でペットを切り替えると静止画と動画で動く形にする。

## 何を

- `assets/pets/rinrin/`: `spritesheet.webp`（`build_pet`・cellScale 2）・`pet.json`（`clips`・`wakeWord`「りんちゃん」）・`clips/`（13 本 317 コマ・6.5MB）・`clips.json`・`SOURCE.md`（元素材の在処と作り直し方）
- `.steering/20261027-rinrin-clips/`: brief と識別子契約・1〜3 回目の検分表・道具（`align_poses.py`・`run_comfy.py`・`inspect_clips.py`）・指示文（`run1〜3.json`）・`clips-src.json`。元素材（zip・姿勢画像・元の動画・workflow・証拠のスクショ）は `.git/info/exclude` で外している
- `scripts/verify/pet-switch.sh`: `TGT`（ビルドの target）と `SWITCH_TO`（切り替え先）を環境変数で指定できるようにした（既定は今までどおり）
- `docs/architecture.md` §4.4 にりんりんを追記、§4.5 の見出しにりんりんを足した
- テスト 2 本（`pet-core`）: りんりんの `pet.json`（clips・wakeWord・cellScale）、同梱 2 体の `clips.json` が 13 本・同じ名前・コマ数が `clips/<名前>/` の WebP の数と合うこと（コマ数を 1 つ変えると落ちることを確認）

## 確かめたこと

- Comfy Cloud 無料枠 5 回のうち 3 回を使用（4 本・8 本・なでられ 3 候補）。残り 2。課金・アカウント作成なし
- 15 本すべてで顔・模様・輪郭が保たれ、始点差 2.6〜3.2・終点差 4.8〜6.1（まめドラゴンの 1 回目と同水準）。なでられは 3 本とも手の混入なし
- K 判定: 1 回目の 4 本（まめドラゴンとの比較つき）→「OK。続けて」
- `TGT=target-rinrin SWITCH_TO=rinrin bash scripts/verify/pet-switch.sh`: まめドラゴン起動 → りんりんへ切り替え（`(rinrin) loaded ... clips 13`）→ state.json に rinrin の 3 項目 PASS。切り替え後に `clip idle loaded`・`clip grabbed loaded` がログに出た
- `bash scripts/gate.sh` GATE OK（`CARGO_TARGET_DIR=target-rinrin`）

## 残り・未確認

- 画面での動画の見え方は、まめドラゴンと同じ 3/4 の解像度上限（`pets::max_cell`）のまま。K の「画質はこれ以上良くならないか」への答えは tasklist の「止まった点」に（上限を上げる案と 960×960 で作り直す案）
- K の実機（常駐のペット）での切り替えは未確認（試験用のデータフォルダで並走して確認した）
- マージは K の判断

🤖 Generated with [Claude Code](https://claude.com/claude-code)
