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
- 2026-10-21 kuroko/claude-opus-5-5（同上・評価係の指摘で本物の拡張の状態を撮り直して発見）: code-implementation.md の「浮いた下端の操作」を訂正（隙間はバー自身の bottom を負にして塞ぐ。擬似要素を下へ延ばすと短いカードでもスクロールバーが出てボタンの文字が折れる）＋キー表示はスクロールバーで狭い時だけ消す（コンテナクエリ）＋select の「すべてのタグ ⌄」は field-sizing: content
- 2026-10-21 kuroko/claude-opus-5-5（同上・訂正）: 浮いた下端の操作の隙間は「バー自身を動かす」案を撤回（止まっていない時の余白が 16px 変わり承認の見た目からずれた）→ 層はバーの中・隙間はバーの外側の影（不透明）で塗る、に訂正
- 2026-10-21 kuroko/claude-opus-5-5（同上・受入係の追加判定から）: キー表示の手当てを「コンテナクエリ」から「1 行に固定してキーは折り返した先で見えなくする（高さ 54・overflow hidden）」へ訂正（fr・de・zh_TW は通常幅でも折れていた）。はみ出し検査はテキストの行しか見ないのでボタンの高さを全言語で見る
- 2026-10-21 kuroko/claude-opus-5-5（同上・持ち主の判定から）: キー表示は「一列のどれか 1 つでも入らなければ列ぜんぶ消す」（組としてそろえる・ResizeObserver で測る）を追記
- 2026-10-20 kuroko/claude-opus-5-5（memoro のストア画像の見出し）: marketing-pages.md のグラデーション見出しに「明るい地では濃い停止色（#0071e3→#8e44ec→#d6336c・撮影画素で ≥4.35:1）」と「中央寄せの見出しは幅 100% でなく縮めて置く（グラデーションが文字の端から端に乗る）」を追記
- 2026-10-20 kuroko/claude-opus-5-5（memoro の LP 9 言語を v0.4.0 に追随）: code-implementation.md に「節の光の疑似要素の横あふれは section に overflow-x: clip（375px で 38px）」と「折り返しより下の節を撮るときは lazy の画像を eager＋decode、浮いたナビを隠す」を追記
- 2026-10-20 kuroko/claude-opus-5-5（memoro の LP の機能の見出しをグラデーション文字に）: marketing-pages.md に「19px・600 の小さい見出しは大きい文字に当たらない＝停止色はすべて 4.5:1 以上。白なら #0071e3/#8e44ec/#d6336c で可、#f5f5f7 では青が 4.31」を追記
