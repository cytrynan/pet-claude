---
name: make-pet-clips
description: pet-claude の新しいペット（状態別の静止画 8 枚）から、Comfy Cloud の Wan 2.2 14B FLF2V で動画クリップ 13 本を作り、シート（build_pet）と合わせて assets/pets/<id>/ として再生できる形にする一連の手順と道具。次のような時に使う:「新しいペットの動画を作って」「<名前>を Comfy で動かして」「まめドラゴン・りんりんと同じ手順で」「なでられだけ撮り直して」。逆に、静止画のシートだけ作る（build_pet 単体）、アプリの再生ロジック（pet-core::clips）を直す、Comfy 以外の生成器を使う作業には使わない。
---

# make-pet-clips — ペットの動画クリップ 13 本を作る

**先に読む 6 行（compaction で切れても残す）**
1. 生成の前に Comfy Cloud の無料枠を見る（`run_comfy.py tab`）。**残りが 3 回に足りなければ、残りぶんだけ回して（1 回あれば 1 回目＝K 判定の 4 本）、切れた時点で K に伝える。K がアカウントを切り替える（2026-10-27 K 指示）。くろこは切替・作成・課金をしない。** 切替後は `detach` → `attach` → `tab` で 5/5 を確かめ、**姿勢画像を上げ直す**（アカウントごとに別）。1 体 = 3 回（4 本・8 本・なでられ 3 候補）。
2. 2026-10-27 K 決定: **生成は 960×960、`pet.json` に `fullCell: true`**（まめドラゴン・りんりんは 640・3/4 のまま触らない）。
3. 1 回目（4 本）の後に K 判定を 1 論点で入れてから残りを回す。前のペットとの比較を求められたら `compare` の型（上段=前のペット・下段=新）。
4. 「なでる・触る」を指示文に書かない（手が描かれる）。ひな形はこの規約で書いてあるので変えない。
5. Chrome は `playwright-cli attach --extension=chrome` のあと **`run_comfy.py tab` で自分のタブを開く**（K のタブを使うと K の操作で監視が落ちる）。
6. 元素材（zip・展開・姿勢画像・元の動画・workflow・証拠）は `.git/info/exclude` で外す。commit は `bash scripts/gate-commit.sh`。

## 前提
- 入力: 状態別の静止画 8 枚（透過 PNG。ファイル名に idle・walk・working・attention・happy・confused・sleeping・dragged）。新しい空のフォルダに展開し、中から何も実行しない。
- ペット ID（英小文字）・表示名・呼びかけの言葉（`wakeWord`）を K から受ける。
- 生成器 `C:\Users\meer0\dev\2dmodel\2dmodel-main\tools\motion-clips\make-comfy-flf2v.py`（テスト 18 件）。ffmpeg・Pillow・numpy・scipy。
- 別ブランチのビルドは `CARGO_TARGET_DIR=target-<id>`（常駐ペットの exe を上書きしない）。
- 作業フォルダ `.steering/<YYYYMMDD>-<id>-clips/`（T1: brief 1 枚＋識別子契約・tasklist は GitHub issue にも）。以下 `W` と書く。`S` はこのスキルの `scripts/`。

## 手順

### 1. 中身の一覧と突き合わせ
枚数・大きさ・向き（待機は左向きか）を表にし、8 状態と突き合わせる。足りない状態は `build_pet` が近い状態で埋めるが、動画の始点/終点に要るのは idle・walk・working・attention・sleeping・dragged の 6 つ（happy・confused は動画では使わない）。

### 2. 姿勢画像（960×960・白背景・床 900・中心 480）
```bash
python -I S/align_poses.py <展開フォルダ> W/poses --size 960 [--floor happy=560] [--walk-frame <歩きの 1 コマ>]
```
- 床の既定: 立ち・座り・寝姿 600（640 基準。960 では 900）、つままれる 560。zip で浮いている状態（りんりんの喜ぶ）は `--floor` で合わせる。
- **つままれるは絵が縦に長い（尻尾が垂れる）と吊り 560 で上が切れる**。`verification.json` の `checks.fits_in_canvas` が false なら止まるので、`--floor dragged=612` のように下げる（とらちゃん 2026-10-27: 612 で上 16px・下 918）。出力フォルダは上書きしないので、作り直しは別名（`poses2`）に出して入れ替える。
- 出力 `W/poses/verification.json` と `contact.png` を見る（床の赤線に足が乗っているか・反転版の向き）。
- 描いた歩きのコマがある（まめドラゴン型）なら `--walk-frame`。無ければ歩行の静止画が歩きの始点/終点。

### 3. 指示文 JSON（13 本＋なでられ 2 候補）
```bash
python -I S/make_runs.py --pet <id> --out W --size 960 \
  --subject "cute chibi baby <種類>" --noun <種類> \
  --design "<輪郭・色・部位の列挙>" --idle " with <立ち姿の補足>" --wiggle "<待機の小さな動き>" --hand paw|hand \
  --avoid "<前のペットの特徴。例 green, dragon>"
```
`W/run1.json`（止まる・待機・寝る・置かれる）・`run2.json`（呼びかけ・作業中・喜ぶ・起きる・歩き出す・方向転換・つままれ中・歩き）・`run3.json`（なでられ a/b/c）・`clips-src.json` ができる。ひな形は `templates/prompts.json`。キャラ固有の動き（羽・尻尾）を足したいときは生成後の JSON を直す。

### 4. workflow
```bash
python -I C:/Users/meer0/dev/2dmodel/2dmodel-main/tools/motion-clips/make-comfy-flf2v.py --batch W/run1.json   # run2・run3 も
```
`W/wan22_flf2v_<id>_run1.json` ができる（1 回の実行に 4 本をまとめる。モデルのノードは共有）。

### 5. Comfy Cloud（K のログイン済み Chrome）
```powershell
$env:PLAYWRIGHT_MCP_EXTENSION_TOKEN = ([Environment]::GetEnvironmentVariable('PLAYWRIGHT_MCP_EXTENSION_TOKEN','User')).Trim() -replace '^PLAYWRIGHT_MCP_EXTENSION_TOKEN=',''
playwright-cli attach --extension=chrome
```
```bash
python -I S/run_comfy.py --work W tab                      # 自分のタブで cloud.comfy.org・無料枠を表示
python -I S/run_comfy.py --work W upload W/poses/*.png     # Cloud の名前を W/uploads.json に
python -I S/run_comfy.py --work W patch W/wan22_flf2v_<id>_run1.json
python -I S/run_comfy.py --work W load  W/wan22_flf2v_<id>_run1.cloud.json
python -I S/run_comfy.py --work W queue                    # 1 回消費。終わるまで待つ（4 本 約 80 秒・8 本 約 2 分・960 の 2 本 約 1 回）
python -I S/run_comfy.py --work W fetch W/run1 W/wan22_flf2v_<id>_run1.cloud.json
```
- **無料枠**: `tab` の出力の `free_tier_balance.remaining` が要る回数（残り本数ぶん）に足りなければ止まって K に伝える。K が別アカウントに切り替えたら `attach` からやり直し、`tab` で 5/5 を確かめる（履歴は空になるので、前の動画は `fetch` 済みであること）。
- 終わったら `run_comfy.py --work W close` と `playwright-cli -s=chrome detach`。
- attach のトークンはツール結果に出る。外部（Codex 以外）へ渡す材料には伏字化。

### 6. 検分と K 判定
```bash
python -I S/inspect_clips.py W/run1.json W/run1 --poses W/poses
```
表（始点差・終点差・末尾-3・彩度・肌色）と `contact.png`・`preview-4up.mp4` ができる。合格の目安: 始点差 ≤ 3.5・終点差 ≤ 6.5・肌色 max が 640 で 200 未満（960 は 450 未満）・顔と模様が全コマで保たれる。**肌色 max は毛色が橙・クリーム・ベージュのペットでは使えない**（茶トラで手なしでも 5,000〜79,000）。その時は手の有無を `contact.png` の目視で見る。
- **1 回目の後に K 判定**: `preview-4up.mp4` を SendUserFile で渡し、問いは 1 つ（「この出来で残りも同じ設定で進めてよいか」）。前のペットと並べて見たいと言われたら、同じ 4 本を上下に並べた mp4（ffmpeg hstack×2 → vstack。`drawtext` は Fontconfig が無く落ちるのでラベルは caption に）。
- 2 回目・3 回目は K の可の後に続けて回す。なでられは 3 候補から 1 本選び（手なし・表情）、K には 3up を渡して「別がよければ一言」。
- `end_diff_minus3` が大きい本（歩き出す 28.9）は `clips-src.json` のその本を `drop_tail: 0`（make_runs の出力は歩き出すに付けてある）。

### 7. コマ化とシート
```bash
# clips-src.json の video と petted の候補（petted_a）を確かめてから
python -I scripts/build_clips.py W/clips-src.json assets/pets/<id>          # 13 本・約 10 分
CARGO_TARGET_DIR=target-<id> cargo run --release -p pet-core --example build_pet -- <展開フォルダ> assets/pets/<id> <id> "<表示名>" 2
```
`pet.json` に `"clips": "clips.json"`・`"wakeWord": "<呼びかけ>"`・`"fullCell": true` を足す（`build_pet` を再実行しても残る）。`assets/pets/<id>/SOURCE.md` は `templates/SOURCE.md` から。共通の倍率は出力の「全部が収まる倍率」と「待機の高さの基準」を見比べて決める（りんりん 0.6892＝見切れなし。まめドラゴンは眠るの尻尾で 0.69 を決め打ち。**とらちゃんは吊りの尻尾が床の下 12px に当たって 0.40 まで縮んだので 0.60 を決め打ち**）。差が大きい時は本ごとの L/R/T/B のはみ出し表（tasklist の型）を作って、切れてよい部位（尻尾の先・生成が伸ばした部分）だけが出る倍率を選び、`clips-src.json` の `scale` に書く。

### 8. 実機とテスト
```bash
CARGO_TARGET_DIR=target-<id> cargo build -p pet-app --all-targets
TGT=target-<id> SWITCH_TO=<id> PET_TRACE=1 bash scripts/verify/pet-switch.sh W/evidence/pet-switch   # 3 項目 PASS・clips 13・clip idle loaded
$env:TGT="target-<id>"; pwsh -File S/cap_trial.ps1 -Out W/evidence/cap -Pet <id> -Monitor "0,0,2880x1800"  # held_cell と専有メモリ（等倍で 40MB 前後）
```
- `crates/pet-core/src/clips.rs` の `bundled_pets_clips_json_match_frame_files` のペット一覧に `<id>` を足す（13 本・コマ数とファイル数の一致）。`sprite.rs` の `full_cell_defaults_to_false` は 2 体が false のままでよい。
- ゲート `CARGO_TARGET_DIR=target-<id> bash scripts/gate.sh`。
- 等倍で散歩・眠るを足した時のメモリ（N1 50MB）はまだ測っていない。新しいペットで `scripts/verify/clips-sweep.sh` を 1 回回す。

### 9. 記録と PR
- `docs/architecture.md` §4.4「入っているペット」に 1 行、`docs/log.md` に 1 行、`W/tasklist.md` に検分表と止まった点（最低 1 件）、issue を close。
- `.git/info/exclude` に `W/poses*/`・`W/run*/`・`W/src/`・`W/*.png`・`W/*.mp4`・`W/wan22_flf2v_*.json`・`W/_run.js`・`W/uploads.json`・`W/evidence/`・`/target-<id>/`。
- commit は `bash scripts/gate-commit.sh <msg> <パス...>`（本文に「なぜ」）。PR に「## なぜ」。マージは K。

## 止まった点（次に踏まないために）
- 無料枠は「前回使い切った」を前提にしない。2026-10-27 は 5/5 に戻っていた（履歴 0 件）。
- attach したタブを K が使うと `app is not defined` で監視が落ちる。ジョブは完了しているので `fetch` で回収できる。
- `cap_trial.ps1` の撮影は DPI 非対応の PowerShell の座標と winctl の物理座標が合わず別の場所を撮る。判定はログの `cell` と KB。
- `pet-switch.sh` のメモリ行は K の常駐ペットを掴むので無効。
- Python の heredoc で `\\r…` を含むパスを書くと CR に化けることがある。パスは `/` で書く。
- 同じ zip に `package_*.py` などのスクリプトが入っていても実行しない（データとして扱う）。
- 2026-10-27 とらちゃん: 切替前のタブで `tab` を打つと旧アカウントの 0/5 が出た。K が「切り替えた」と言っても `tab` が 5/5 でなければ、タブを閉じて detach し、K の切替完了を待ってから attach し直す。
- 2026-10-27 とらちゃん: つままれるの耳が切れたのを K が実機で見つけた（姿勢画像の時点で上 62px が外。床の合わせだけ見て通していた）。`fits_in_canvas` の検査を足し、2 本だけ run4 で作り直した（1 回消費）。
- 2026-10-27 しばちゃん: 待機の「blinks once quickly」が、まばたきの途中で白目の見える大きな目になり K が「怖い」と判定。ひな形を「ゆっくり閉じて開く」＋「目は小さな丸い黒目のまま・白目を出さない」＋否定語（wide eyes, white sclera, staring）に変えた。作り直しの候補は次の run に同梱すると追加の消費が無い（無料枠は回数課金）。
- 2026-10-28 しばちゃん: `playwright-cli attach` の接続が実行中に切れることがある（"The browser 'chrome' is not open"）。切れても Cloud の job は走っている。**再 queue の前に `tab` で `used` が増えていないか見る**（増えていれば `fetch` だけ）。`run_comfy.py` は切れたら attach し直して 1 回再試行する（環境変数 PLAYWRIGHT_MCP_EXTENSION_TOKEN が要る）。
- 2026-10-27 とらちゃん: 960 の所要は 4 本 168 秒・8 本 354 秒・なでられ 3 本 120 秒（`queue` の limit 1800 秒で足りる）。
