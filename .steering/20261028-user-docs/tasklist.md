---
type: Steering
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-28T04:10:00+09:00 }
---

# 利用者向け文書 3 点（README・docs/usage.md・PDF マニュアル）（T1）

## brief
- K 指示（2026-10-28）「最後に README の更新、使い方の更新（両方簡潔にわかりやすく）、マニュアル（PDF でチャット、音声入力、設定画面、基本的な機能）を作成して」。親セッション（fix/remove-wake-setting）から分かれた側セッションで作る。
- いまの README は「予定（未実装）」にチャット・読み上げ・音声入力が残り、ペットが 4 体のままなど実態（6 体・Phase 3〜4 実装済み・ウェイクワードは保留で設定の欄も外す）と食い違う。
- 成果物: ①`README.md`（できること→使い方→技術の詳細→ライセンス。使い方は `docs/usage.md` への導線だけ） ②`docs/usage.md`（起動・操作・Claude Code 連携・チャット・声・設定・ペット切替・困ったとき。1 操作 1 行） ③`docs/manual/pet-claude-manual.pdf`（8 章。原稿 `manual.html`＋生成 `build-pdf.js`＋画像 `images/`。実機のスクリーンショット）。
- 画像の撮り方: 承認カード・質問カード・バー・右クリックメニューは自分の worktree の exe を `PET_CLAUDE_DATA_DIR` を分けて起こし `winctl shot` で撮る（`capture-shots.sh`）。設定画面とチャット欄は、入力の無い試験プロセスからは前面を取れない（`render-ui.js` の注記）ので Edge で `ui/*.html` を描いて撮る（`capture-ui.js`）。設定画面の HTML は fix/remove-wake-setting 側（欄を外した後の形）を読むだけで使う。
- 触らない: `ui/settings.html`・`crates/pet-app/src/state.rs`（親が同時に編集中）、main の作業ツリーと exe、常駐ペット。commit は `gate-commit.sh` だけ。main へのマージはしない。
- 文章は `stop-ai-slop-jp`・`technical-writing` に沿う。文書の内容は実装（architecture.md・ui/*.html・main.rs）から読み取れることだけ。

## タスク
- [x] README.md を実態に合わせて書き直す (#122)
- [x] docs/usage.md を書く (#123)
- [x] 実機・Edge のスクリーンショットを撮り images/ に入れる（1 枚 500KB 以下） (#124)。実機 6 枚（pet-idle・pet-bar・pet-menu・perm-card・ask-card・status-bubble）＋ Edge 5 枚（settings・chat-empty・chat-answer・chat-menu-mode・chat-menu-model）。最大 185KB
- [x] マニュアルの原稿と PDF 生成スクリプトを作り、PDF を目視する (#125)。11 ページ・8 章・画像 13 か所・目次の内部リンク 24・フォント埋め込み（check-pdf.py OK）。全ページを 60〜75dpi で描いて目視
- [x] gate-commit → PR（マージは K） (#126)。commit 60f16ec・PR #128（マージは K）
- [x] K 指示（2026-10-28）「マージの前にマニュアルを作り直す。私が細かいところを修正しやすいように Word で作り直して」: 正本を `pet-claude-manual.docx` に変え、PDF は Word（COM）で書き出す形にした。`manual.html`・`build-pdf.js` は削除。初版を組む `build-docx.js`・書き出す `export-pdf.ps1` を追加。12 ページ・8 章・画像 13 か所・目次の内部リンク 8・フォント埋め込み（check-pdf.py OK）。全ページを 60dpi で描いて目視（司令塔セッション）

- [x] K 指示（2026-10-28）「apple 風のデザインを採用して。文字は可能な限り減らし、図や色分けで見やすく」: apple-design（HIG）の値で docx を組み直し。章番号は tint の小さな数字＋26pt の題、罫線は髪の毛、「項目: 内容」の表は灰色チップ、状態は色つきチップ（言葉つき）。図 8 枚（仕組み・マウス・放置・状態・承認・チャット・音声入力・ペット）を `diagrams.html` に HTML で書き `render-diagrams.js` で Edge から 2 倍 PNG に。文章は箇条書きを表と図に置き換えて約半分に。10 ページ・各章 1 ページ・check-pdf.py OK・全ページ目視。apple-design の改善点は references/print-documents.md に追記（テンプレ同期済み）

## 止まった点
- 最初の撮影は K のデスクトップの中身（開いていた表）が背景に写った。無地の下敷き窓を「最前面でない」形で出したら、入力の無いプロセスからは前に出せず他の窓の裏に出た（実測）。最前面＋物理座標指定（`backdrop.ps1`）に直して撮り直した。ペット（1 秒ごとに最前面を取り直す）と吹き出し（後から出る）はその上に載る
- 設定画面の実機撮影は、自分の worktree の exe にまだ「呼びかけで音声入力を始める」の欄がある（親が別 PR で外している途中）ので、欄を外した後の `ui/settings.html`（fix/remove-wake-setting 側）を Edge で描いて撮った。設定画面とチャット欄はもともと実機では撮れない（フォーカスを持つ窓は試験のプロセスから前に出せない。`render-ui.js` の注記）
- PDF の flex の行（図＋文）はページをまたげず丸ごと次ページへ行き、3 章が半ページ空いた。図を float にして文を回り込ませた。Chromium の PDF の Type3 フォント（basefont が空）は字形を直接書いたもので、埋め込み判定では埋め込み扱いにした

- Word 版（docx）: docx-js が Normal（標準）スタイルを書かないので、日本語の Word が自前の既定（テーマの本文フォント＝游明朝）で補い、docDefaults の Yu Gothic UI が負けた。Normal スタイルを明示して解決（Word COM の `Font.NameFarEast` が「+本文のフォント - 日本語」を返すので見つかった）
- Word 版: 表の中の図の段落に keepNext を付けると、Word が表の行同士を連結して表ごと次ページへ落ちる。表は行の分割禁止だけにし、keepNext は本文に直接置く図だけにした。1 章は図の行が入りきらず次ページへ落ちたので、図を少し縮めて 1 ページに収めた

- Apple 風: 説明文の無い図に keepNext を付けると、次の見出し（keepNext）→次の図…と連結して章の中身が丸ごと次ページへ落ちた。keepNext は説明文つきの図だけに
- Apple 風: 本文の行間（1.42 倍）が図の段落にも掛かり、図の行が 1.4 倍の高さになっていた。図の段落だけ行間 1 倍に。Word の列の高さは COM の `Range.Information(6)`（ページ上の y）で測ると見積もりの誤りが分かる
- Apple 風: Noto Sans JP（Windows 同梱の可変フォント）は Word が解決できず Calibri に落ちる。本文も図も Yu Gothic UI に統一

## 振り返り
- 計画どおり 3 点とも作った。README は 49 行（旧 60 行）。usage.md 131 行。マニュアル 11 ページ
- 文書の内容は architecture.md・ui/*.html・main.rs・tts.rs・voice.rs から読み取れることだけを書いた。未確認の挙動は書いていない
