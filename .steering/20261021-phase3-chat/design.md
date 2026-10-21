---
type: Steering
description: Phase 3 前半のチャットの設計（claude -p を子プロセスで起こす）
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-21T21:55:00+09:00 }
---

# 設計: チャット（`claude -p` を子プロセスで起こす）

一次資料（2026-10-21 に確認）と手元の実測 `evidence.md`（#7）: https://code.claude.com/docs/en/cli-reference の CLI flags、https://code.claude.com/docs/en/authentication の「Authentication precedence」。

## 1. なぜ SDK のライブラリでなく `claude -p` か

Agent SDK は TypeScript と Python のライブラリで、中で動いているのは Claude Code と同じ仕組み。pet-app は Rust なので、SDK を使うには Node か Python の常駐プロセスが要り、普段のメモリ（N1）を食う。`claude -p` は同じ仕組みを 1 回ごとに起こして終わるので、**使っていない間は何も常駐しない**。SDK への移行が要るのは、答えを少しずつ流したい・途中で人に聞き返したい時だけ（今回はどちらも要らない）。

## 2. 起動の形

作業フォルダは `%LOCALAPPDATA%\pet-claude\chat\`（空のフォルダ。リポジトリや K の作業フォルダの CLAUDE.md・設定を拾わない）。

```
claude -p --output-format json
       --safe-mode
       --model sonnet
       --tools "" --disallowedTools "mcp__*" --strict-mcp-config
       --settings "{\"disableAllHooks\":true}"
       --disable-slash-commands
       --permission-prompts none
       --max-turns 1
       --system-prompt-file <アプリの資源>\chat-system.txt
       [--resume <前回の session_id>]
       （本文は標準入力で渡す。コマンドラインに載せない＝引用符の罠と、プロセス一覧から見えるのを避ける）
```

| 引数 | 狙い | 根拠 |
|---|---|---|
| `--safe-mode` | K の `~/.claude/CLAUDE.md`・自動メモリ・スキル・プラグイン・hooks・MCP を読まない（認証はそのまま）。**無いと K の個人の指示書がペットの会話に毎回入る**（入力 19,156 → 787 トークン、evidence E8・E9） | `--help`「Start with all customizations ... disabled」。実測 E6・E9 |
| `--model sonnet` | **CLI 既定（Fable）は K の上限に達すると全く答えない**（E2。`--fallback-model` も効かない）。秒数は haiku と変わらない（起動が約 3 秒を占める） | 実測 E2・E3・秒数表。要求の非スコープ「モデルの選択」は画面での選択のこと。固定値は定数 1 か所 |
| `--tools ""` | 組み込みツールを全部外す | cli-reference「Use `""` to disable all」 |
| `--disallowedTools "mcp__*"`・`--strict-mcp-config` | MCP のツールも外す（`--tools` は MCP に効かない） | 同「The flag doesn't affect MCP tools」 |
| `--settings {"disableAllHooks":true}` | K の `~/.claude/settings.json` に入れた pet の hooks が、チャット自身の会話で鳴らないようにする（A4） | `--settings` は同じキーを上書き。**`disableAllHooks` の効き方は実機で確かめる**（効かなければ §5 の予備策） |
| `--permission-prompts none` | ツールが無いので出ないはずだが、出ても待たずに拒否 | 同。v2.1.259 以降 |
| `--max-turns 1` | ツールなしなので 1 往復で終わる。暴走の上限 | 同 |
| `--system-prompt-file` | 「デスクトップのペットとして、日本語で短く（目安 2〜3 文）答える。コードやコマンドは書かない」。既定の Claude Code の指示（コーディング用）を丸ごと置き換える | 同「Use a replacement flag when the surface, identity, or permission model differs」 |
| `--resume <id>` | 会話の続き（A5）。初回は付けない。出力 JSON の `session_id` を覚える | 同 |

`--bare` は使わない（bare はサブスクのログインを読まない。authentication「Bare mode does not read CLAUDE_CODE_OAUTH_TOKEN」）。

## 3. 認証の切り替え

| 設定 | 子プロセスの環境 |
|---|---|
| サブスク（既定） | 親の環境から `ANTHROPIC_API_KEY`・`ANTHROPIC_AUTH_TOKEN`・`CLAUDE_CODE_USE_BEDROCK`・`CLAUDE_CODE_USE_VERTEX`・`CLAUDE_CODE_USE_FOUNDRY`・`ANTHROPIC_PROFILE` を**消す**（ほかに `CLAUDECODE`・`CLAUDE_CODE_ENTRYPOINT` も消す＝pet-app を Claude Code の中から起動した時に入れ子と誤認させない）。`claude -p` は API キーが環境にあれば必ずそれを使う（authentication「In non-interactive mode (-p), the key is always used when present」）ので、消さないと K が気づかないまま API 課金になる |
| API キー | 上と同じものを消したうえで、資格情報マネージャーから読んだ鍵を `ANTHROPIC_API_KEY` に入れる |

- 環境の組み立ては純粋関数 `pet_core::chat::child_env(mode, parent_env, key) -> Vec<(String, String)>` にしてテストする（A1・A2）
- 鍵の保存は `CredWriteW`（種類 GENERIC、名前 `pet-claude/anthropic-api-key`）。設定画面は「保存済み」とだけ表示し、鍵を読み返して表示しない。消すボタンあり
- `state.json` に足すのは `chat_auth: "subscription" | "api_key"` の 1 項目だけ（無い・知らない値ならサブスク）
- 設定画面に「いまの接続を確かめる」ボタン: 同じ環境で `claude auth status` を走らせ、`authMethod`（`claude.ai` / `api_key` など）を表示する

## 4. アプリ側の流れ

- `chat_submit(text)` は待たずに戻り、別スレッドで子プロセスを起こす（`CREATE_NO_WINDOW`）。ペットは「考え中」（作業中の動き）
- 同時に 1 本だけ。待ち中の送信は「まだ考え中です」で断る。120 秒で子プロセスを止めて時間切れ
- 終わったら JSON を読む: `is_error`・`result`・`session_id`。答えはメインスレッドへ渡して吹き出しに出す
- 吹き出しには `sanitize_line` を通した先頭 140 字（N3）。全文はチャット欄の下に出す（「続きを見る」）
- `claude` の場所: PATH の各フォルダで `claude.exe` を探し、無ければ同じフォルダの `claude.cmd` の隣の `node_modules\@anthropic-ai\claude-codein\claude.exe`（npm 版。evidence 冒頭）、最後に `%USERPROFILE%\.localin\claude.exe`（ネイティブ版）。**`.cmd` は直接起こさない**（cmd.exe を挟むと `--settings` の JSON の引用符が cmd の解釈を受ける）。見つからなければ「Claude Code が見つかりません」
- 失敗の見分け: 終了コードが 0 でも 1 でも、標準出力が JSON なら `is_error`・`result` を見る（上限・鍵違いは `is_error: true` で `result` に理由。E2・E11）。JSON でなければ標準エラーの先頭行を理由にする（E12）
- アプリ終了・「新しいチャット」・時間切れでは子プロセスを `TerminateProcess` で止める。子は Job Object に入れ、pet-app が落ちても道連れにする（A7）
- ログには「送った字数・かかった秒数・終了コード・is_error」だけ。本文と答えは書かない（N4）

## 5. 未確定と予備策

- ~~`disableAllHooks` が効かなかった場合の予備策~~ → **不要**: `--settings {"disableAllHooks":true}` でも `--safe-mode` でも hooks は鳴らない（E6）。両方付ける
- `--permission-prompts` が入っていない古い CLI: 2.1.305 では通る（E1）。引数エラー（E12）なら理由をそのまま吹き出しに出し、ログに CLI の版を書く（再試行はしない＝外すと安全側の引数が消えるため）
- 起動の速さ: サブスクで 5.0〜7.6 秒（sonnet）、4.7〜5.2 秒（haiku）。起動が約 3 秒で、モデルでは縮まない（evidence の秒数表）。目安 5 秒はわずかに超えるが、考え中の動きで待たせる

## 6. テストの置き場所

- `crates/pet-core/src/chat.rs`（純粋: 引数の組み立て・環境の組み立て・出力 JSON の解析・吹き出し用の切り詰め）と `crates/pet-core/tests/chat.rs`。テスト先行体制の対象（入出力が明確）
- 実機・故障注入: `scripts/verify/chat-fault.py`（偽の `claude.exe` で遅延・異常終了・壊れた JSON を返す）
