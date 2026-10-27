---
type: Steering
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-27T12:50:00+09:00 }
---

# りんりん（パンダ）の動画クリップ（Comfy Cloud・T1）

tracking: #49

## brief
- K 指示（2026-10-27）: 2 体目のペット「りんりん」のパンダのイラスト（`C:\Users\meer0\Desktop\パンダ03_状況別イラスト_20261027.zip`）から、まめドラゴンと同じ手順で ComfyUI（Comfy Cloud の Wan 2.2 14B FLF2V）の動画を作る。司令塔のセッションから委任。
- 入力: zip は 8 状態（待機・歩行・作業中・呼びかけ・喜ぶ・困る・眠る・つままれる）各 1024×1024 の透過 PNG。zip の時点で頭の幅（約 560px）と床（y=900。喜ぶ 840・つままれる 820）がそろっている（zip の README・`verification.json`）。まめドラゴンの 8 状態と 1 対 1 で、足りない・余る状態は無い。歩きの 8 コマ（まめドラゴンの walk00）は無いので、歩きの始点/終点は `02-walk.png` を使う。
- 構成: まめドラゴンと同じ 13 本（ループ 4: idle・walk・working・grabbed／つなぎ 7: stop・walk_start・turn・sleep・wake・put_down・call／単発 2: happy・petted）。
- 費用: Comfy Cloud の無料枠（実行回数）。2026-10-27 12:35 の `/api/features`（`free_tier_balance`）と画面は **残り 5/5**（前回 2026-10-21 に 4 回使ったはずだが、`/api/jobs` の履歴も 0 件。枠が戻ったのか別アカウントかは不明＝K に報告）。課金・アカウント作成はしない。割り振り: 1 回目＝試し 4 本、2 回目＝なでられ以外の 8 本、3 回目＝なでられ（手が入らない指示文・seed 2 通り）、残り 2 回＝撮り直し。
- 前の作業: `../20261021-wan-walk-trial/`・`../20261021-wan-transitions/`・`../20261021-clip-playback/`。生成器は `C:\Users\meer0\dev\2dmodel\2dmodel-main\tools\motion-clips\make-comfy-flf2v.py --batch`。

## 識別子契約
- ペット ID `rinrin`・表示名「りんりん」・フォルダ `assets/pets/rinrin/`
- 姿勢画像（`poses/`・640×640 白背景・中心 x=320・共通倍率 0.675＝待機の高さ 480）: idle / idle_right / walk / working / working_right / attention / happy / confused / sleeping / sleeping_right / dragged。床は y=600、喜ぶ・つままれるは 560
- 出力の接頭辞: `video/rinrin_<動き>`。Comfy Cloud は上げた画像の名前をハッシュに変えるので、workflow の LoadImage は上げた後の名前に差し替える（`run_comfy.py`）
- 元素材（zip・`src/`・`poses/`・`run*/`・workflow JSON）はリポジトリに入れない（`.git/info/exclude`）
- ビルドは `CARGO_TARGET_DIR=target-rinrin`

## タスク
- [x] zip を展開して中身を一覧にし、まめドラゴンの 8 状態と突き合わせる（1 対 1・差分なし）
- [x] 無料枠を確認する（残り 5/5。履歴 0 件の食い違いは K へ報告）
- [x] 姿勢画像をそろえる（`align_poses.py`・11 枚・床 600/560・中心 319.5〜320.5・待機 481px）
- [x] 1 回目: 止まる(33f)・待機(81f)・寝る(49f)・置かれる(33f) を 1 回で実行 → K 判定（4up の mp4） (#50)
  - [x] 実行（2026-10-27 12:41・約 80 秒・4 本とも成功・無料の残り 4/5・`run1/`）
  - [x] K 判定（`run1/preview-4up.mp4`・`run1/contact.png`・まめドラゴンとの比較 `run1/compare-mame-rinrin.mp4`）→ 2026-10-27「OK。続けて」
- [x] 2 回目: 呼びかけ・作業中・喜ぶ・起きる・歩き出す・方向転換・つままれ中・歩き（8 本） (#51)
  - 2026-10-27 12:52・約 122 秒・8 本とも成功・残り 3/5（`run2/`）。8 本とも顔・模様・輪郭が保たれる。歩きは足が交互に出てその場で歩く。方向転換は背中を見せて回る
- [x] 3 回目: なでられ（反応だけの指示文・seed 2 通り）→ 手の混入を検分 → K 判定 (#52)
  - 2026-10-27 13:05・3 本とも成功・残り 2/5（`run3/`）。3 本とも手なし（上端の肌色 114〜119）。始点差 2.6〜2.9・終点差 4.8〜5.7。採用 a（`rinrin_petted_a`）。K には `run3/preview-3up.mp4` を渡し、別がよければ変える
- [x] 検分（始点一致・終点のずれ・最後のコマの彩度落ち・手の混入）を表にする (#53)（1〜3 回目の表は本ファイル。`inspect_clips.py`）
- [x] `clips-src.json` → `scripts/build_clips.py` → `assets/pets/rinrin/clips/`・`clips.json` (#54)
  - 13 本 317 コマ・6.5MB（まめドラゴン 361 コマ 7.7MB）。共通の倍率 0.6892＝全部が収まる倍率と同じ（見切れなし。まめドラゴンは眠るの尻尾で 0.69 を決め打ちしたが、パンダは尻尾が短い）。姿勢合わせの倍率は 1.000〜1.002（Cloud の出力が姿勢画像と同じ大きさ）。確認 `clips-contact.png`
- [x] `build_pet` でシート（`spritesheet.webp`・`pet.json`）を作り、`clips`・`wakeWord` を足す (#55)
  - `spritesheet.webp` 3072×4576（6.1MB・cellScale 2）。`wakeWord` は「りんちゃん」（K の呼び方）。確認 `sheet-preview.png`
- [x] 設定でペットを「りんりん」に切り替えて静止画と動画が再生されるか確かめる（スクショか実機） (#56)
  - 2026-10-27 13:11 `TGT=target-rinrin SWITCH_TO=rinrin PET_TRACE=1 bash scripts/verify/pet-switch.sh`（試験用のデータフォルダ・K の常駐と並走）: 起動まめドラゴン → りんりんへ切り替え（`(rinrin) loaded in 3304 ms ... clips 13`）→ state.json に rinrin の 3 項目 PASS。切り替え後に `clip idle loaded`・`clip grabbed loaded`・`show -> idle` がログに出た（動画が読まれて流れている）。証拠 `evidence/pet-switch/`（`pet-after.png` にパンダ、`settings.png` に設定画面）。メモリの行は K の常駐 pet-app を掴んだ値で無効（スクリプトは最初の pet-app を取る）
- [ ] `SOURCE.md`・`docs/architecture.md`・`docs/log.md`・PR `feat/rinrin-clips`（マージは K） (#57)

## 1 回目の検分（くろこ・`inspect_clips.py`）
- 4 本とも顔・耳・模様・輪郭が保たれ、始点から終点の姿勢へ移っている（`run1/contact.png`）。待機は 20 コマ目で 1 回まばたき（口が開く）。寝るは 24 コマ目で伏せの姿勢に入り、最後まで眠った顔。
- 始点の一致（姿勢画像との平均差）2.75〜3.16、終点 4.83〜5.68（まめドラゴンの 1 回目は始点 2.6〜3.2・終点 4.9〜6.0＝同水準）。
- 彩度は中盤 110〜126 → 最後 3 コマ 90〜116 で落ちる（まめドラゴンと同じ。`build_clips.py` が末尾 3 コマを捨てる）。
- 上端の肌色画素は最大 155（手の混入なし。まめドラゴンで手が入った回は 12574）。

| 本 | コマ | 始点差 | 終点差 | 彩度(中) | 彩度(最後3) | 肌色max |
|---|---|---|---|---|---|---|
| rinrin_stop | 33 | 3.16 | 5.29 | 125.7 | 115.6 / 105.7 / 105.0 | 121 |
| rinrin_idle | 81 | 2.75 | 5.68 | 110.0 | 105.6 / 103.5 / 102.7 | 115 |
| rinrin_sleep | 49 | 2.84 | 4.88 | 119.4 | 106.5 / 89.6 / 105.4 | 107 |
| rinrin_put_down | 33 | 3.14 | 4.83 | 115.3 | 106.9 / 105.8 / 105.5 | 155 |

## 2 回目の検分（くろこ）

| 本 | コマ | 始点差 | 終点差 | 終点差(末尾-3) | 彩度(中) | 彩度(最後3) | 肌色max |
|---|---|---|---|---|---|---|---|
| rinrin_call | 49 | 2.80 | 4.78 | 12.64 | 118.2 | 116.0 / 103.3 / 102.2 | 134 |
| rinrin_working | 81 | 2.69 | 5.67 | 6.43 | 100.4 | 77.7 / 76.7 / 76.4 | 119 |
| rinrin_happy | 49 | 2.56 | 4.97 | 4.43 | 113.8 | 108.9 / 91.1 / 104.8 | 145 |
| rinrin_wake | 49 | 2.85 | 5.73 | 12.2 | 115.3 | 103.6 / 91.0 / 100.6 | 112 |
| rinrin_walk_start | 33 | 3.14 | 5.28 | **28.91** | 121.6 | 121.8 / 110.1 / 115.6 | 133 |
| rinrin_turn | 33 | 3.11 | 4.76 | 4.14 | 117.0 | 109.4 / 109.1 / 109.1 | 118 |
| rinrin_grabbed | 49 | 2.76 | 4.95 | 7.65 | 111.6 | 103.6 / 90.9 / 97.2 | 177 |
| rinrin_walk | 81 | 2.91 | 6.14 | 22.38 | 125.6 | 113.4 / 99.3 / 106.2 | 156 |

- 歩き出す（walk_start）は終点の 3 コマ前で姿勢がまだ届いていない（28.9）→ まめドラゴンと同じく `drop_tail: 0`（末尾を捨てない）。歩き（loop）の 22.4 は足の途中なので問題ない。
- 作業中は最後の 3 コマで彩度が 100 → 77 に落ちる（ノート PC の白が広い分）。末尾 3 コマを捨てるので採用に影響なし。

## 止まった点
- （作業中に追記）
