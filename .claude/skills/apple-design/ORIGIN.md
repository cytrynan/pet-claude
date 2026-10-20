# ORIGIN
- 取得元: https://github.com/SudewaJay/apple-design-skill @ 57bfab7daf8766a9a45cfc405790e749c71eda30（2026-10-18 取得・MIT）
- 収録: apple-design/（SKILL.md + references/6本）と LICENSE を無改変で収録（release/*.skill は同内容のzipのため除外）
- 中身は取得時に全文確認済み（設計仕様の文書のみ・実行コードなし）

## ローカル改良（上流には無い。追記した事実をここに1行ずつ）
- 2026-10-20 kuroko/claude-opus-5-5（memoro の UI 磨き込みで判明）: references/code-implementation.md に「Contrast trap」注記と「Local refinements」節（色付きガラスの主ボタン・グレーのガラス・下端の浮いた操作バーのエッジ効果・進捗バー・撮影画素でのコントラスト実測）を追加／references/hig-foundations.md の Contrast 行を WCAG の「大きい文字」の定義（18pt 以上、または 14pt 以上の太字）に訂正
- 2026-10-20 kuroko/claude-opus-5-5（memoro の評価ボタン P3）: code-implementation.md の Local refinements に「主の1つだけを色付きにした選択肢の組」（同じ大きさ・19px 太字・補足は上の行に 13px で外出し）を追加
- 2026-10-20 kuroko/claude-opus-5-5（memoro のフォント F1）: SKILL.md の Font strategy に「日本語／CJK UI」の1項（system-ui を CJK 名より前に置かない・Noto Sans JP を含む指定順・palt は短い UI 文字だけ・96 DPI の既知の問題）を追加
- 2026-10-20 kuroko/claude-opus-5-5（memoro のフォント F1・K 指定の組み合わせ）: SKILL.md の日本語／CJK UI の項に「太字だけ別の書体にする合成フォント（@font-face＋local()＋font-weight 範囲）」と「可変フォントが -Thin と報告される罠」を追記
- 2026-10-20 kuroko/claude-opus-5-5（memoro の完了画面 P4）: SKILL.md の日本語／CJK UI の項に「フォントを変えたら固定余白の SVG 文字（グラフの目盛り）を撮り直す」を追記
- 2026-10-20 kuroko/claude-opus-5-5（memoro のオプション O1・O2）: code-implementation.md の Local refinements に「操作つきの一覧の行（⋯メニュー・行クリックで編集・3段の行）」と「本題が下に埋もれたページの見出しにセグメント」を追加
- 2026-10-20 kuroko/claude-opus-5-5（memoro の設定 O3）: code-implementation.md の Local refinements に「設定行（時刻のカプセル・外部設定へ行く ↗ の行・日本語の脚注に text-wrap: pretty）」を追加
- 2026-10-20 kuroko/claude-opus-5-5（memoro のデータ入出力 O5）: code-implementation.md のセグメントの値を訂正（ダークの選択中は #636366。--bg-cell では見分けられない）＋2〜3択の排他選択に使う旨
- 2026-10-20 kuroko/claude-opus-5-5（memoro の保存ダイアログ C1）: code-implementation.md の「撮影画素で確かめる」に「撮る前に出現アニメーションを止める・ビューポートを撮影範囲以上に」を追加
- 2026-10-20 kuroko/claude-opus-5-5（memoro のトースト C2）: code-implementation.md の Local refinements に「任意のページに出すトースト（日本語）」（角丸 22・380px・auto-phrase と balance を併用しない・表示時間と一時停止）を追加
- 2026-10-20 kuroko/claude-opus-5-5（memoro の LP L1）: marketing-pages.md のグラデーション見出しに「価格・機能の見出しに青→紫→ピンクはテンプレ臭（レビューで外した）。白が既定」を追記／code-implementation.md の撮影の注意に「headless は OS の配色に従う→宣伝画像は配色を明示」を追加
- 2026-10-20 kuroko/claude-opus-5-5（同上・訂正）: marketing-pages.md のグラデーション見出しの注記を訂正（「レビューで外した」は誤り。持ち主が残す判断をした＝持ち主の判断事項として提案に留め、勝手に外さない）
- 2026-10-20 kuroko/claude-opus-5-5（同上）: marketing-pages.md のグラデーション見出しの注記に「持ち主によっては積極的に使う→入れられる所を探して提案」と「白い節のガラスのカードは背後に薄い色のにじみ＋本文の灰色を一段濃く（#5f5f64）」を追記
- 2026-10-20 kuroko/claude-opus-5-5（memoro のボタン文字の太さ）: code-implementation.md の Contrast trap に「標準の太さにすると 3:1 の特例が消える→青を濃く（ライト #0064cc・ダーク #006cda で 4.6:1 以上）」を追記
- 2026-10-21 kuroko/claude-opus-5-5（memoro の UI 磨き込みの実装・本物の拡張での検証）: code-implementation.md に3点を追加 — ①⋯メニューの z-index は固定のガラスの見出し・上端のぼかしより下 ②全体の `button:active` の縮みが行のボタンにも掛かる→行は縮めず薄く塗る ③グレーのガラスの列の背後のにじみは左右に出さない（scrollWidth が増えて全言語のはみ出し検査に掛かる・blur は数えない）
