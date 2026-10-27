# マニュアルの作り方

`pet-claude-manual.pdf` は `manual.html` から作る。画像は `images/` にある実機のスクリーンショット（1 枚 500KB 以下）。

## 文章だけ直すとき

1. `manual.html` を直す。
2. `node docs/manual/build-pdf.js` で PDF を作る（Edge が A4 に印刷し、日本語フォントを埋め込む）。
3. `python docs/manual/check-pdf.py` で、ページ数・章の見出し・フォントの埋め込み・目次のリンクを確かめる（`OK` で終わる）。
4. PDF を開いて目視する。

## 画像を撮り直すとき

前提: `cargo build -p pet-app --all-targets` と `cargo build -p pet-hook` が済んでいること。常駐のペットは止めなくてよい（試しのペットは `PET_CLAUDE_DATA_DIR` を分けて起こし、自分が起こした PID だけ止める）。

| 画像 | 撮り方 |
|---|---|
| ペット・バー・右クリックメニュー・承認のカード・質問のカード・固定文言の吹き出し | `bash docs/manual/capture-shots.sh target/manual-raw`。背景に無地の最前面の窓（`backdrop.ps1`）を敷いて撮る |
| 設定画面・チャット欄・切替のメニュー | `node docs/manual/capture-ui.js target/manual-ui [設定画面の HTML のフォルダ]`。Edge で `ui/*.html` を描く（入力の無い試験のプロセスからは、フォーカスを持つ窓を前に出せないため） |

撮ったら `python docs/manual/finish-images.py target/manual-raw target/manual-ui docs/manual/images` で余白を切り、500KB 以下にして `images/` へ入れる。

- `find-pet-window.ps1 <pid>`: 試しのペットの窓の矩形（物理座標）。`winctl find` は最初に見つかった PetClaudeWindow を返すので、常駐のペットと区別するために PID で引く
- 撮った画像に K のデスクトップの中身や秘密（パス・鍵）が写っていないか、入れる前に見る
