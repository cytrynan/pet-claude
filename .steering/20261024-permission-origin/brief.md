# 承認のカードの「ターミナルで」を来た先で変える（T1 brief）

## 要求（2026-10-24。司令塔のセッションから委任）

承認のカード（「許可」「拒否」「ターミナルで」）について、要求がターミナルの Claude Code（CLI）から来たのか、Claude デスクトップアプリ（Code タブ）から来たのかを判定できるか調べる。できるなら、CLI は「ターミナル」、アプリは「Claude アプリ」と出し分ける。できないなら根拠と代案を報告して止まる。

## 結論

判定できる（実測。公式の仕様としての保証は一部だけ）。文言は 3 通り。

| 来た先 | 決め方 | ボタン | 注記 |
|---|---|---|---|
| ターミナル | `CLAUDE_CODE_ENTRYPOINT=cli` | ターミナルで | 全文が見えないので、許可はターミナルで |
| Claude アプリ | `claude-desktop`／`claude-desktop-3p`／`local-agent` かつ `CLAUDE_CODE_EXECPATH` ＝ `CLAUDE_PID` の実行ファイル | Claude アプリで | 全文が見えないので、許可は Claude アプリで |
| 分からない | 上以外（入れ子・`-p`・VS Code・古い pet-hook） | Claude Code で | 全文が見えないので、許可は Claude Code で |

## 実測（2026-10-24・Claude Code 2.1.305〜2.1.308・デスクトップ 3.6556.2）

hook に `node` の書き出しスクリプトを登録し、hook が受け取る環境変数・標準入力・親プロセスを記録した。CLI はアプリの環境を継がないよう、WMI（`Win32_Process.Create`）で起こした（`set` で CLAUDE 系の変数が 0 件であることを先に確認）。

| 起こし方 | `CLAUDE_CODE_ENTRYPOINT` | `CLAUDE_CODE_EXECPATH` | `CLAUDE_PID` の実行ファイル |
|---|---|---|---|
| デスクトップアプリの Code タブ（PostToolUse の hook） | `claude-desktop` | アプリ同梱の `claude.exe` | 同じ（親はアプリ本体 `AnthropicClaude\app-3.6556.2\claude.exe`） |
| ターミナルの対話 `claude`（SessionStart の hook） | `cli` | 無し | — |
| ターミナルの `claude -p` | `sdk-cli` | 無し | — |
| アプリのセッションの Bash から `claude -p`（入れ子） | `claude-desktop`（**引き継ぐ**） | アプリ同梱の `claude.exe`（引き継ぐ） | npm の claude（別） |

- 標準入力の JSON に来た先を示す欄は無い（`session_id`・`transcript_path`・`cwd`・`permission_mode` など）
- Claude Code は `CLAUDE_CODE_ENTRYPOINT` が既にあれば上書きしない（入れ子の行）。だから環境変数だけだと、アプリの中から起こした CLI をアプリと誤る。`CLAUDE_CODE_EXECPATH` はアプリが付ける印（CLI 単独では付かない）で、hook を起こした Claude Code の実行ファイルと照合して入れ子を弾く
- 組み込んだ pet-hook を偽のアプリに向けて 5 通りで起こし、送られた `origin` を確かめた: きれいな環境の `cli` → `terminal`／このデスクトップのセッションの環境そのまま → `desktop_app`／デスクトップの環境で `CLAUDE_PID` だけ別の実行ファイル → `unknown`／`sdk-cli` → `unknown`／変数なし → `unknown`。どれも終了コード 0・出力 `{}`

## 公式の記載

- `CLAUDE_PID`: 記載あり。「hook commands」に Claude Code 自身の pid を渡す（code.claude.com/docs/en/env-vars）
- `app.entrypoint` の値（`cli`・`sdk-cli`・`sdk-ts`・`sdk-py`・`claude-vscode` など）: 監視のドキュメントに記載あり。デスクトップの起動経路 `claude-desktop`・`claude-desktop-3p`・`local-agent` と、「入れ子の子は除く」も同じページにある（code.claude.com/docs/en/monitoring-usage）
- `CLAUDE_CODE_ENTRYPOINT`・`CLAUDE_CODE_EXECPATH` が環境変数として渡ること: **記載なし**（env-vars のページ全文を grep）。実測だけが根拠。変わったら「Claude Code で」に落ちるので、誤った場所は指さない

## 変更

- `pet_core::origin`（判定と文言。テスト 6 件）・`WireEvent.origin`（省略可）・pet-hook が承認の時に付ける・アプリが `askLabel`・`note` をカードへ渡す
- カードのボタン行を折り返さない・余白を詰める（「ほかに N 件」＋「Claude アプリで」＋拒否＋許可で折り返した。`render-ui.js` に承認のカード 5 通りと左右のはみ出し検査を追加）

## 残り

- 実機（本物のマウス）での確認は K の了承待ち
- K の hooks は `pet-claude-main` の pet-hook、常駐のアプリは `pet-claude-feat-phase3-voice` のビルド。どちらもこの変更を含まないので、今のカードは「ターミナルで」のまま。アプリだけ新しくなると、古い pet-hook は `origin` を付けないので「Claude Code で」になる。両方を作り直して初めて出し分けが効く
