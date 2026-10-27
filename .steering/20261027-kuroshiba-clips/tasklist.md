---
type: Steering Tasklist
description: ペット 4 体目「しばちゃん」（黒柴・id kuroshiba）の動画クリップ 13 本と assets/pets/kuroshiba（T1: brief＋識別子契約）
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-27 }
---

# しばちゃん（黒柴）の動画クリップ 13 本 — tracking #69

## brief
- 入力: `C:\Users\meer0\Desktop\黒柴06_状況別イラスト_20261027.zip`（1024×1024 透過 PNG 8 枚＋originals・verification.json・README。接地 6 枚は下端 y900、喜ぶ 840、つままれる 820）。展開 `.steering/20261027-kuroshiba-clips/src/`（git には入れない。中の `package_shiba_states.py` は実行しない）
- 作り方: スキル `make-pet-clips`（960×960・`fullCell: true`）。とらちゃんの学び（収まり検査・倍率の見比べ・肌色 max は目視で補う）を適用
- 1 回目（4 本）の後に K 判定。無料枠は開始時点 2/5（とらちゃんのアカウントの残り）なので 1・2 回目に使い、3 回目（なでられ）の前に K の切替を待つ
- 成果: `assets/pets/kuroshiba/` 一式、`clips.rs` の一覧に `kuroshiba`、`docs/architecture.md` §4.4 に 1 行、PR（base main・マージは K）

## 識別子契約
| 項目 | 値 |
|---|---|
| ペット ID | `kuroshiba` |
| 表示名 / 呼びかけ（`wakeWord`） | しばちゃん / しばちゃん |
| フォルダ | `assets/pets/kuroshiba/` |
| ブランチ / worktree | `feat/kuroshiba-clips`（main ed5ecf7 から）/ `C:\Users\meer0\dev\pet-claude\pet-claude-feat-kuroshiba-clips` |
| ビルド | `CARGO_TARGET_DIR=target-kuroshiba` |
| 作業フォルダ W | `.steering/20261027-kuroshiba-clips/` |
| 生成サイズ / コマ | 960×960 / `fullCell: true`・cellScale 2 |
| 動画名 | `kuroshiba_<動き>` |

## tasks
- [x] 素材の展開と姿勢画像（960・収まり検査） (#70)
- [x] 指示文 JSON（run1〜3）と workflow 3 本 (#71)
- [x] Comfy Cloud 1 回目（4 本）と K 判定 (#72) — K「待機の目が怖い」→ 候補 3 本から c を採用
- [x] Comfy Cloud 2 回目（8 本＋待機 3 候補）・3 回目（なでられ 3 候補） (#73)
- [~] コマ化（倍率の確認）・シート・pet.json（fullCell true） (#74)
- [ ] 実機・テスト・ゲート (#75)
- [ ] 記録・commit・PR（base main） (#76)

## 検分表
### run1（2026-10-27・無料枠 2/5 → 1/5・163 秒）
| 本 | コマ | 始点差 | 終点差 | 終点差(末尾-3) | 彩度(平均) | 肌色max |
|---|---|---|---|---|---|---|
| kuroshiba_stop | 33/33 | 3.56 | 4.69 | 5.27 | 70.9 | 364 |
| kuroshiba_idle | 81/81 | 2.52 | 5.88 | 4.35 | 71.1 | 213 |
| kuroshiba_sleep | 49/49 | 3.41 | 5.59 | 4.83 | 75.5 | 190 |
| kuroshiba_put_down | 33/33 | 3.46 | 5.01 | 4.26 | 68.7 | 318 |

止まるの始点差 3.56 は目安 3.5 をわずかに超えるが、歩行の静止画（描いた歩きのコマが無い）からの開始で、りんりん・とらちゃんと同じ傾向。肌色 max は黒毛なので有効（全部 450 未満＝手なし）。目視でも手なし。

### run2（2 回目 8 本＋待機の候補 3 本・1/5 → 0/5・586 秒）
| 本 | コマ | 始点差 | 終点差 | 終点差(末尾-3) | 彩度(平均) | 肌色max |
|---|---|---|---|---|---|---|
| kuroshiba_call | 49/49 | 3.17 | 5.61 | 13.26 | 73.6 | 277 |
| kuroshiba_working | 81/81 | 2.89 | 5.72 | 6.29 | 77.1 | 206 |
| kuroshiba_happy | 49/49 | 2.83 | 5.48 | 4.5 | 69.7 | 298 |
| kuroshiba_wake | 49/49 | 3.36 | 5.84 | 5.88 | 74.0 | 307 |
| kuroshiba_walk_start | 33/33 | 3.45 | 5.22 | 27.64 | 73.9 | 459 |
| kuroshiba_turn | 33/33 | 3.48 | 4.32 | 3.74 | 72.0 | 217 |
| kuroshiba_grabbed | 49/49 | 3.2 | 5.19 | 5.3 | 67.1 | 341 |
| kuroshiba_walk | 81/81 | 2.88 | 6.04 | 10.73 | 73.7 | 303 |
| kuroshiba_idle_a（まばたき無し seed 1234） | 81/81 | 3.23 | 5.44 | 6.78 | 69.0 | 200 |
| kuroshiba_idle_b（まばたき無し seed 7） | 81/81 | 2.87 | 5.55 | 4.01 | 68.7 | 185 |
| kuroshiba_idle_c（ゆっくり閉じる seed 42） | 81/81 | 2.61 | 5.87 | 4.68 | 70.1 | 205 |

11 本とも始点差 ≤ 3.5・終点差 ≤ 6.5。歩き出すの肌色 max 459 は目安 450 をわずかに超えるが目視で手なし（茶の足先）。K は待機に **c** を選んだ（2026-10-27）。

### run3（なでられ 3 候補・K 切替後のアカウント 5/5 → 3/5）
| 本 | コマ | 始点差 | 終点差 | 終点差(末尾-3) | 肌色max |
|---|---|---|---|---|---|
| kuroshiba_petted_a | 49/49 | 2.89 | 5.31 | 4.27 | 3661 |
| kuroshiba_petted_b | 49/49 | 3.08 | 4.81 | 3.62 | 246 |
| kuroshiba_petted_c | 49/49 | 2.99 | 5.45 | 4.14 | 207 |

a は 24 コマ目に耳の輪郭が二重になる揺れがあり（肌色 max 3661 もそこ）、**b を採用**（手なし・終点差が最小・目を細めて笑う）。c も可。
同じ run を 2 回実行してしまった（下の止まった点）ので 5/5 → 3/5。

## 止まった点
- 2026-10-27: K「左から 2 番目、目が怖いので作り直して」＝待機（preview-4up は 1×4 の横並び。報告で 2×2 と書いたのは誤り）。原因は指示文の「blinks once quickly」で、まばたきの途中（コマ 16・32）に白目の見える大きな目になっていた。直し: まばたきを外す／目は小さな丸い黒目のまま・白目を出さないと書く／否定語に wide eyes, white sclera, staring を足す。候補 3 本（a・b=まばたき無し seed 1234/7、c=ゆっくり閉じるまばたき seed 42）を 2 回目の 8 本と同じ 1 回に同梱（無料枠は回数課金なので追加コスト無し）。
- 2026-10-27/28: 3 回目で `playwright-cli attach` の接続が途中で 2 回切れた（"The browser 'chrome' is not open"）。1 回目は queue のクリック後・監視の前に切れ、「実行していない」と誤解して attach し直して再 queue → 同じ job を 2 回回し 1 回無駄にした（5/5 → 3/5）。2 回目は fetch の途中。教訓: 切れた後は **先に `tab` で `features` を見て used が増えていれば再 queue しない**。`run_comfy.py` の `pw()` に「not open なら attach し直して 1 回だけ再試行」を入れた。

## 振り返り
（計画と実績の差）
