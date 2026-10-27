# マニュアルの作り方

正本は Word の `pet-claude-manual.docx`。K が Word で直す。`pet-claude-manual.pdf` はそこから書き出した配布用。画像は `images/` にある実機のスクリーンショット（1 枚 500KB 以下）。

## 文章を直すとき

1. `pet-claude-manual.docx` を Word で直して保存する。
2. `pwsh docs/manual/export-pdf.ps1` で PDF を書き出す（Word の COM。見出しからしおりを作り、フォントを埋め込む）。
3. `python docs/manual/check-pdf.py` で、ページ数・章の見出し・フォントの埋め込み・目次のリンクを確かめる（`OK` で終わる）。
4. PDF を開いて目視する。

`build-docx.js` は docx の初版を組んだスクリプト。**走らせると K の手直しを上書きする**ので、構成を丸ごと作り直す時にだけ使う（`node docs/manual/build-docx.js`。docx は npm のグローバルを使う）。

Word で直す時の約束:

- 見出し 1（章）と見出し 2（節）は Word の組み込みスタイル。目次と PDF のしおりはここから作る
- 図と説明文を横に並べている所は、罫線無しの 1 行の表。表の行は分割しない設定にしてあるので、図だけが次ページへ落ちる時は図を小さくする
- 等幅（Cascadia Mono）の部分はコマンドやパス。日本語の既定は Yu Gothic UI（Normal スタイルに明示。無いと日本語の Word が游明朝で補う）

## 画像を撮り直すとき

前提: `cargo build -p pet-app --all-targets` と `cargo build -p pet-hook` が済んでいること。常駐のペットは止めなくてよい（試しのペットは `PET_CLAUDE_DATA_DIR` を分けて起こし、自分が起こした PID だけ止める）。

| 画像 | 撮り方 |
|---|---|
| ペット・バー・右クリックメニュー・承認のカード・質問のカード・固定文言の吹き出し | `bash docs/manual/capture-shots.sh target/manual-raw`。背景に無地の最前面の窓（`backdrop.ps1`）を敷いて撮る |
| 設定画面・チャット欄・切替のメニュー | `node docs/manual/capture-ui.js target/manual-ui [設定画面の HTML のフォルダ]`。Edge で `ui/*.html` を描く（入力の無い試験のプロセスからは、フォーカスを持つ窓を前に出せないため） |

撮ったら `python docs/manual/finish-images.py target/manual-raw target/manual-ui docs/manual/images` で余白を切り、500KB 以下にして `images/` へ入れる。差し替えた画像は Word 側で「図の変更」で入れ替える（`build-docx.js` を走らせると文章の手直しが消える）。

- `find-pet-window.ps1 <pid>`: 試しのペットの窓の矩形（物理座標）。`winctl find` は最初に見つかった PetClaudeWindow を返すので、常駐のペットと区別するために PID で引く
- 撮った画像に K のデスクトップの中身や秘密（パス・鍵）が写っていないか、入れる前に見る
