---
type: Reference
description: JinTanba/embed-pet（Apache-2.0・Web アプリにペットを埋め込む Agent Skill）の実装解析。素材の規約とペットの切り替えについて pet-claude が取り込む点と見送る点
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-21T16:40:00+09:00 }
stale_after: 2027-04-21
---

# embed-pet の実装解析（2026-10-21）

対象は JinTanba/embed-pet の main（コミット `a600563`、2026-06-17。コミットはこの 1 件だけで、★1・fork 0）。読み取り専用で解析した。clone 先はセッションの一時領域で、スクリプトは実行していない。参照はリポジトリ内の相対パス。pet-claude 側の行番号は 2026-10-21 時点のもの。

## 1. 何をするものか

| 項目 | 中身 | 根拠 |
|---|---|---|
| 正体 | Claude Code などから `/embed-pet` で呼ぶ Agent Skill。**開発中の Web アプリ**に、Codex 互換のペットを「入手 → ブラウザで確認 → 組み込み」まで入れる | `README.md` 1-8・22-36、`skills/embed-pet/SKILL.md` 1-11 |
| 対象 | Web アプリ（React、または素の HTML/JS）。デスクトップ常駐は無い | `SKILL.md` 17-27 |
| 言語 | スクリプトは TypeScript（Bun）。同梱の hatch-pet は Python（Pillow） | `README.md` 154-159、`vendor/hatch-pet/scripts/*.py` |
| 描画 | canvas 2D。毎コマ `clearRect` してから `drawImage` でセル 1 つを切り出す。`requestAnimationFrame` で状態ごとの fps に合わせて進める | `assets/templates/react/Pet.tsx` 72-84、`assets/templates/vanilla/pet.js` 38-49 |
| 状態と動き | 9 状態。idle / running-right / running-left / waving / jumping / failed / waiting / running / review。散歩や睡眠のような自律行動は無く、`state` を外から変えるだけ | `Pet.tsx` 11-18、`pet.js` 54-57 |
| AI エージェントとの連携 | **ペットの動きとエージェントの状態はつながっていない**（「`state` prop を変えるのは開発者の仕事」と明記）。エージェントを使うのは、組み込み作業そのもの（Skill）と、Codex SDK でのペット生成だけ | `SKILL.md` 103-104、`scripts/engine/pet-runner.ts` 12-21・50-60 |
| 素材の形式 | `pet.json` ＋ `spritesheet.webp`（1536×1872・8 列×9 行・1 コマ 192×208・透過必須）。`pet.json` は名前などだけで、コマ数も fps も持たない | `README.md` 42-55、`vendor/hatch-pet/references/codex-pet-contract.md` 3-33 |
| ペットの切り替え | 実行中の切り替え画面は無い。組み込む前に petdex のカタログから選ぶか、hatch-pet で生成し、プレビューで承認したものを 1 匹だけ置く。実行中に変えられるのは状態と `src`（React の prop） | `SKILL.md` 29-74、`Pet.tsx` 27-35・86-89 |
| ライセンス | 本体は Apache-2.0。`vendor/hatch-pet/` は openai/skills から同梱した Apache-2.0（`LICENSE.txt` 付き） | `LICENSE`、`README.md` 161-166、`vendor/hatch-pet/LICENSE.txt` |

入手の経路は 2 つある。

- (A) petdex カタログ: `https://petdex.dev/api/manifest` を取得して検索し、`pet.json` と `spritesheet.webp` を落とす。WebP のヘッダ（VP8X / VP8 / VP8L）を自前で読んで大きさを確かめる（`scripts/engine/petdex-source.ts` 12-22・86-116・122-167）。投稿者のファンアートでライセンスが機械可読でないため、使う前に `submittedBy` を見せて利用者に確認させる（`SKILL.md` 49-58）
- (B) hatch-pet で生成: 隔離した `CODEX_HOME` を作り（`auth.json` はシンボリックリンク）、`@openai/codex-sdk` で hatch-pet を無人実行する（`scripts/engine/codex-home.ts` 28-67、`pet-runner.ts` 12-21）。作者の実測で `low` でも 30 分超、`medium` で 60 分超、Codex の枠を使う（`references/engine-generation.md` 33-37）

## 2. 素材の規約（embed-pet と pet-claude の比較）

| 項目 | embed-pet（Codex 契約） | pet-claude（現状） |
|---|---|---|
| シート | 1536×1872 固定・9 行。PNG か WebP（`codex-pet-contract.md` 5-10） | 9 行（Codex）と 11 行（行 9 睡眠・行 10 つままれ）の両方を読む。行数は高さ÷208（`crates/pet-core/src/sprite.rs` 113-116） |
| コマ数 | **実行時にアルファから数える**。各行を左から見て、アルファ 10 超の画素がある間を数え、最初の空セルで止める。最低 1（`Pet.tsx` 53-70、`assets/preview/index.html` 59-76） | **コードに固定**。Codex の値（6/8/8/4/5/8/6/6/6、独自行 6/4）（`sprite.rs` 40-60） |
| 速さ | 状態ごとの fps を再生側に持つ（idle 3、running 8 など）。コマが少ないシートは 1 周が短くなる（`Pet.tsx` 20-23） | 状態ごとの 1 周の時間 `cycle_ms` を持ち、コマ数で割る（`sprite.rs` 90-95） |
| 行が無いとき | 考慮なし。空の行は 1 コマ扱いで、透明なセルを描き続ける（`Pet.tsx` 68 の `Math.max(n, 1)`） | 行数が足りなければ代替（睡眠→待機を半分の速さ、つままれ→待機の行）。**行はあるが中身が空、は見ていない**（`sprite.rs` 62-80） |
| 未使用セル | 完全に透明。透明画素の RGB も 0 にする（`compose_atlas.py` 110-118、`validate_atlas.py` 105-108・128-132） | 組み立て（作業中の `compose.rs`）は縮小を事前乗算で行い、戻すときにアルファ 0 の画素を `[0,0,0,0]` にしている。同等 |
| 1 コマへの収め方 | `paste_centered`: 1 枚ずつ縮小してセルの**中央**に置く（`compose_atlas.py` 60-66）。足元はそろわない。そのため QA で「大きさの跳ね」「足元の跳ね」を見て、`stable-slots` で直す運用（`qa-rubric.md` 38・55） | 作業中の `compose.rs` は全ポーズに同じ倍率を掛け、**足元を下端中央にそろえる**。embed-pet より良い。変える必要は無い |
| 左向き | 右向きを反転してよいのは、模様や小物の左右が意味を変えない時だけ。反転はコマごとに行い、コマの順序は保つ（hatch-pet `SKILL.md` 95・258） | `RowSpec.mirror` で反転（作業中の `compose.rs`）。静止画 1 枚なので順序の問題は無い |
| 行の意味 | `running` は「作業中」で、足で走る絵にしない。`waiting` は承認や入力を待つ姿勢。`idle` は動きの少ない呼吸とまばたき（`animation-rows.md` 19-29） | 同じ解釈で使っている（Agent の作業中→Running、入力待ち→Waiting） |
| `pet.json` | `id`・`displayName`・`description`・`spritesheetPath`。フォルダ名が id（`codex-pet-contract.md` 14-35）。コマの大きさは 192×208 に固定 | `id`・`displayName`・`spritesheetPath` に、独自の `cellScale`（コマを 192×208 の何倍で描いたか。1〜4）を足した（作業中の `sprite.rs` の `PetManifest`）。ほかの項目は無視。`spritesheetPath` に従って開く（`crates/pet-app/src/main.rs` 47-51）。`cellScale` が 2 のシートは Codex の大きさと違うので、Codex アプリや embed-pet では読めない |

embed-pet 自身の組み込みとプレビューは `spritesheetPath` を読まず、`spritesheet.webp` か `.png` を決め打ちで探す（`scripts/integrate.ts` 41、`scripts/preview.ts` 25-27）。pet-claude はいまのまま `spritesheetPath` に従うほうがよい。

## 3. まめドラゴンに当てはめた場合

`C:\Users\meer0\Downloads\まめドラゴン.zip` の中身は、状態ごとの静止画 8 枚（7 枚が 1312×1199、つままれだけ縦長の 1199×1312）と README・`verification.json`。README によると足元の位置と表示の大きさはそろえていない。Codex の行の意味（`animation-rows.md` 19-29）に当てると次のようになる。

| 行 | Codex の意味 | まめドラゴンの絵 | メモ |
|---|---|---|---|
| 0 idle | 呼吸・まばたき | 01 待機 | |
| 1 running-right | 右へ移動 | 02 歩く | 絵が右向き |
| 2 running-left | 左へ移動 | 02 歩く（反転） | 左右で意味の変わる模様は無さそう。目視で確認 |
| 3 waving | 挨拶 | 04 呼びかけで代用 | **いまの behavior は Waving を出さない**（`behavior.rs` の `Pose::` は Jumping・Failed・Grabbed・Waiting・Running・Idle・Sleep・RunRight・RunLeft だけ） |
| 4 jumping | 跳ぶ | 05 喜ぶ | クリックと完了の反応 |
| 5 failed | 失敗・しょんぼり | 06 困る | |
| 6 waiting | 入力待ち | 04 呼びかけ | PRD S4 の「こちらを見て小さく手を振る」に近い絵 |
| 7 running | 作業中（走らない） | 03 作業中 | ノート PC の絵で、Codex の意味どおり |
| 8 review | 見直し・考え中 | 03 作業中で代用 | behavior は Review を出さない |
| 9 睡眠（独自） | — | 07 眠る | |
| 10 つままれ（独自） | — | 08 つままれる | 縦長。作業中の `compose.rs` の `fit_scale` でコマに収まる |

作業中の `compose.rs` は、`specs` に渡さなかった行を透明のまま残す（同ファイル 153 行のコメント）。waving と review に絵を渡さない場合、その 2 行は空になる。いまは behavior がその 2 つを出さないので表には出ないが、次の節の取り込みが要る理由はここにある。

## 4. ペットの切り替え

embed-pet に実行中の切り替え画面は無い。参考になるのは次の 3 点。

- **切り替えたらシートを測り直す**。React 版は `src` が変わると effect が走り、画像を読み直してコマ数を数え直す（`Pet.tsx` 86-89 の依存 `[src, scale]`）。pet-claude は行数を窓を作るときに 1 回だけ求め、`Pet.rows` に持っている（`crates/pet-app/src/pet_window.rs` 621-632）。設定で切り替えるときは、シートと一緒に行数（と、取り込むならコマ数）を入れ替え、`set_pose` と同じく再生を最初のコマからやり直す（`pet_window.rs` 151-166）
- **一覧はフォルダから作る**。Codex の約束では `pets/<id>/pet.json` の id はフォルダ名で、アプリはフォルダを見てペットを並べる（`codex-pet-contract.md` 14-35）。pet-claude の `assets/pets/placeholder/` も同じ形なので、まめドラゴンは `assets/pets/mame-dragon/`（id は ASCII、`displayName` を「まめドラゴン」）にすると揃う。設定には `displayName` を出し、`state.json` には id を保存する。`description` を `PetManifest` に足せば一覧の 2 行目に使える（任意）
- **他人の絵を入れる口を作るなら、出どころを見せてから**。embed-pet は petdex のペットを落とす前に投稿者を見せ、確認を取る（`SKILL.md` 49-58）。pet-claude は K 一人で配布しない（PRD 11 行）ので急がないが、利用者のフォルダからペットを読む口を足すときに思い出す

## 5. 取り込み候補

### 推す 1 つ: 行の有無とコマ数を、シートのアルファから決める

embed-pet の「コマ数は実行時にアルファから数え、シートには動きの情報を持たせない」（`README.md` 53-55、`Pet.tsx` 53-70）を取り込み、**中身が空の行は「行が無い」として代替へ回す**処理を足す。embed-pet は空の行を 1 コマとして描き続けるので、そこは真似しない。

理由:

1. pet-claude の窓は layered window で、アルファ 0 の画素へのクリックは OS が背後へ通す（`docs/architecture.md` §3）。空の行を再生すると、ペットが消えるうえに右クリックも届かず、メニューから別のペットへ戻せない（戻す道はトレイだけ）。切り替えの機能を入れた瞬間に、利用者が踏みうる穴になる
2. 1 つの規則で、Codex の 9 行、pet-claude の 11 行、まめドラゴンのような静止画由来のシート、waving と review が空のシートを全部扱える。`pet.json` は Codex 互換のまま、ペットごとの設定を増やさずに済む
3. `pet-core::sprite` の純粋な関数で書け、`cargo test` で網羅できる。走査は 11 行×8 列で、最初の不透明画素で打ち切れるので、読み込みと切り替えのときに 1 回やれば足りる

形の案（実装は本セッションの判断）:

- `SheetLayout { rows, frames: [u32; 11] }` をシートと `PetManifest::cell_size()` から作る（`cellScale` が 2 なら 384×416 ずつ見る。embed-pet は 192×208 決め打ち）。各行を左から見て、しきい値を超えるアルファを持つセルを数え、最初の空セルで止める。0 なら「行が無い」
- `clip_for(rows)` を `clip_for(&layout)` にする。行が無ければいまの代替の連鎖へ。行があれば、コマの長さは Codex の値（`cycle_ms ÷ 既定のコマ数`）のまま、1 周を `コマの長さ × 数えたコマ数` にする（embed-pet の「状態ごとの fps」と同じ考え方）
- テスト: 9 行の Codex シートでいまと同じ値になる（回帰）、空の行が代替に回る、1 コマだけの行は静止画になる、途中に穴がある行は穴の手前まで

### 見送るもの

| 候補 | 根拠 | 見送る理由 |
|---|---|---|
| petdex カタログから取る | `petdex-source.ts`、`SKILL.md` 49-58 | 中身は投稿者のファンアートで、ライセンスが機械可読でない（作者自身が警告している）。ネットワークにも依存する。PRD U3（本番素材の方針）が決まってから考える |
| hatch-pet で動くシートを生成する | `vendor/hatch-pet/SKILL.md`、`references/engine-generation.md` 33-37 | まめドラゴンの静止画を基準絵にすれば 9 行の本物のアニメにできる見込みがある。ただし 30〜60 分超と Codex の枠を使い、`python3` と `jq` も要る。作るのは Codex の 9 行だけで、睡眠とつままれは作らない。U3 の候補として残し、いまは静止画の組み立てで進める |
| canvas の再生器（React と素の JS）と、テンプレートに URL を差し込む `integrate.ts` | `Pet.tsx`、`pet.js`、`integrate.ts` | Web アプリ用。pet-claude は `UpdateLayeredWindow` で描くので使い道が無い |
| Bun のプレビューサーバー | `preview.ts`、`assets/preview/index.html` | 全状態をボタンで切り替えて見る作りは、素材の確認に便利そう。ただ pet-claude には実機の検証ツール（`crates/pet-app/examples/winctl.rs`）があり、別のサーバーを足すほどではない。設定画面に「このペットの全ポーズを見る」を付ける案として覚えておく |
| 1 コマごとに違う長さ（idle は 280・110・110・140・140・320ms など） | `animation-rows.md` 5-15 | pet-claude は 1 周の時間を均等に割る。違いが出るのは idle のまばたきの間くらいで、まめドラゴンは静止画由来なので効かない |
| アトラスの検査（`validate_atlas.py`）を読み込み時の関門にする | `validate_atlas.py` 74-132 | 大きさが 1536×1872 でないと不合格になり、pet-claude の 11 行シートが通らない。不合格で読まないより、推す 1 つの「空の行は代替へ」のほうが利用者に優しい。透明画素の RGB を 0 にする点は、作業中の `compose.rs` が既に満たしている |
| `paste_centered`（1 枚ずつ縮めてセル中央に置く） | `compose_atlas.py` 60-66 | 足元がポーズごとにずれる。作業中の `compose.rs`（共通の倍率と足元合わせ）のほうが良い |
| WebP のヘッダを自前で読む | `petdex-source.ts` 122-167 | `image` crate が読める |

## 6. 気づいた食い違い（pet-claude 側。本解析の範囲外）

- `docs/architecture.md` §5.1 の表は `PermissionRequest` を「NeedsInput（手を振る）」と書くが、behavior は NeedsInput に Waiting の行を出す（§4.2 と `behavior.rs` 242 行）。「手を振る」は PRD S4 の言い回しで、使っている行は Waving ではない。文言を直すかどうかは本セッションの判断
