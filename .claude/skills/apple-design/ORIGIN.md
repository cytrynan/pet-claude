# ORIGIN
- 取得元: https://github.com/SudewaJay/apple-design-skill @ 57bfab7daf8766a9a45cfc405790e749c71eda30（2026-10-18 取得・MIT）
- 収録: apple-design/（SKILL.md + references/6本）と LICENSE を無改変で収録（release/*.skill は同内容のzipのため除外）
- 中身は取得時に全文確認済み（設計仕様の文書のみ・実行コードなし）

## ローカル改良（上流には無い。追記した事実をここに1行ずつ）
- 2026-10-20 kuroko/claude-opus-5-5（memoro の UI 磨き込みで判明）: references/code-implementation.md に「Contrast trap」注記と「Local refinements」節（色付きガラスの主ボタン・グレーのガラス・下端の浮いた操作バーのエッジ効果・進捗バー・撮影画素でのコントラスト実測）を追加／references/hig-foundations.md の Contrast 行を WCAG の「大きい文字」の定義（18pt 以上、または 14pt 以上の太字）に訂正
- 2026-10-20 kuroko/claude-opus-5-5（memoro の評価ボタン P3）: code-implementation.md の Local refinements に「主の1つだけを色付きにした選択肢の組」（同じ大きさ・19px 太字・補足は上の行に 13px で外出し）を追加
