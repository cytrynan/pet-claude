---
type: Steering
description: チャット欄のメニューで会話が一瞬消えるバグ（PR #118 後も残る分）の修正のタスクリスト。行末の番号は GitHub issue
generated: { by: kuroko/claude-fable-5-1, at: 2026-10-28T08:40:00+09:00 }
---

# タスクリスト: 20261028-fix-chat-menu-blink-2

tracking: #129

- [x] 再現を測る: main（41eb422）の exe で、答えが無い時の「自動」「Fable 5.1 高」と、答えが来る瞬間を、上配置・下配置で連写し跳ぶコマ数を数える (#130)
- [x] 直し方を試作して選ぶ: 窓の大きさをチャットの間は固定し、透明な部分を `SetWindowRgn` で窓の外にする手が、透過窓（`WS_EX_NOREDIRECTIONBITMAP`）で描画・クリックともに効くかを実機で測る (#131)
- [x] 直す: チャット欄の窓を開いた時から一番高い形で作り、見えている部分だけを領域にする。高さ・領域の計算の単体テスト (#132)
- [x] 検証: `bash scripts/gate.sh`・`node scripts/verify/render-ui.js`・実機連写（答え無し／答え来る × 上下）で 0 コマ・chat-fault.py (#133)
- [x] docs（architecture §6.1・DESIGN.md・log.md）と PR（main へのマージは K） (#134)

## 振り返り

- 計画との差: 仮説（#118 の「範囲外」2 ケースが K の操作）は当たり。直し方は試作の 1 回目（外から窓を大きくして領域を切る）で効いたので、候補 (b)（WebView2 の子窓のサブクラス化）は別プロセスの窓と分かった時点で捨てた
- 止まった点: ①PowerShell の P/Invoke で `SetWindowPos` の引数名が `h` 重複でコンパイルエラー、DPI aware にしないと座標が 2 倍になった（`SetProcessDPIAware` を足した） ②`EnumWindows` のコールバック（scriptblock → delegate）から `param` の変数が見えず 0 件になった（`$script:` スコープで解決） ③`winctl find` は K の常駐ペット（別モニター）を先に拾う＝試しのペットは `Get-CimInstance` の ExecutablePath で PID を取り、その PID の窓だけ列挙した ④下配置の「閉じる」クリックは島の移動先を読み違えて窓の外を押したが、blur で閉じたので遷移の計測としては成立
- 数値: main は答え無しで開く時 2 コマ跳ぶ（上配置）。修正後は 4 通り×2 配置＋答えが来る瞬間×2 配置で 0 コマ。窓は常に 768x752（200%）。gate 591 PASS・render-ui ALL OK・chat-fault は下記
- chat-fault: 21/23（落ちた 2 つは #118 の時と同じ: 「メニューの値は次の送信から」＝試験の opt: が別スレッドで chat_setup を読む既存の問題、「A2 鍵が無ければ起こさない」＝環境依存。この PR と無関係）
