# evidence の注記（評価係の指摘 4 を受けて）

- `1-click-jump.png` と `2-bar.png` は同じ画像（sha256 一致）。「喜ぶ」の撮影としては無効。HEAD では `sweep-df20ddb/b1-jump-0.3s.png` と `b2-bar-2.0s.png` が別の画像
- `8-grabbed.png` は撮影範囲がずれて何も写っていない（無効）
- `10-memory.txt` の最後のブロックは無効（ファイル末尾の注記）
- `7-autonomous-positions.txt` の `t=Ns` は実時間ではない
- 撮影・ログのうち `sweep-*` 以外は、UI の窓を起動時に全部作り `ui::defer` が同期だった版（ceb8234 より前）で撮った
