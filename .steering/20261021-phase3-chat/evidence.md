---
type: Steering
description: Phase 3 チャットの実測（#7 claude -p の引数・認証・秒数）。design.md の根拠
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-21T22:20:00+09:00 }
---

# 実測: `claude -p` の引数（#7）

環境: Windows 11、Claude Code **2.1.305**（npm 版。`%APPDATA%\npm\claude.cmd` は `node_modules\@anthropic-ai\claude-code\bin\claude.exe` を呼ぶだけ）、サブスク（`subscriptionType: max`）。
作業フォルダは空のフォルダ。子の環境から `CLAUDECODE`・`CLAUDE_CODE_ENTRYPOINT` を消して起動（Claude Code の中から試したため）。

| # | 確かめたこと | 結果 |
|---|---|---|
| E1 | 設計 §2 の引数一式（`--tools ""`・`--disallowedTools "mcp__*"`・`--strict-mcp-config`・`--settings {"disableAllHooks":true}`・`--disable-slash-commands`・`--permission-prompts none`・`--max-turns 1`・`--system-prompt-file`）が通るか | 通る（引数エラーなし）。`--max-turns` は `--help` に出ないが受け付ける |
| E2 | **CLI 既定のモデル**で答えるか | **失敗**: 既定は Fable で、K の Fable の上限に達していた。`is_error: true`・`subtype: "success"`・終了コード 1・`result` に上限の文。`--fallback-model sonnet` でも同じ（上限は「使えない」に入らない） |
| E3 | `--model sonnet` | 答える（`modelUsage` に `claude-sonnet-5-5`） |
| E4 | 出力の文字コード | 標準出力は UTF-8 の JSON 1 つ。標準入力の UTF-8 も読める |
| E5 | A3 ツールなし: 作業フォルダの `secret.txt` を読んでと頼む | 読めない（「開く手段がない」と答え、合言葉を出さない）。`permission_denials: []` |
| E6 | A4 hooks: 作業フォルダの `.claude/settings.json` に SessionStart・UserPromptSubmit・Stop で印を書く hook を置く | 対照（`disableAllHooks` なし）は 3 つとも鳴る。`--settings {"disableAllHooks":true}` で 0。**`--safe-mode` だけでも 0** |
| E7 | A5 続き: 「私の名前は K」→ `--resume <session_id>` で「私の名前は？」 | 覚えている（session_id は同じものが返る） |
| E8 | **K の `~/.claude/CLAUDE.md` と自動メモリが読まれる** | `--system-prompt-file` と空のフォルダでも、新しい会話が K の呼び名を知っていた。入力のキャッシュ作成 **19,156 トークン** |
| E9 | `--safe-mode` を足す（CLAUDE.md・スキル・プラグイン・hooks・MCP を切る。認証はそのまま） | 新しい会話は名前を知らない（アカウントのメールアドレスだけは見える）。入力 **787 トークン** |
| E10 | `claude auth status`（既定で JSON） | 認証の変数を消すと `authMethod: "claude.ai"`・`subscriptionType: "max"`。`ANTHROPIC_API_KEY` を入れると `authMethod: "api_key"`・`apiKeySource: "ANTHROPIC_API_KEY"` |
| E11 | 違う API キー | 3.1 秒で `is_error: true`・`api_error_status: 401`・`result: "Invalid API key · Fix external API key"`・終了コード 1 |
| E12 | 知らない引数 | 終了コード 1・標準エラーに `error: unknown option '...'`（JSON なし） |

## 秒数（送ってから答えの JSON が出るまで。サブスク）

| 条件 | 秒 |
|---|---|
| `--safe-mode` なし・sonnet・初回 | 8.2 |
| `--safe-mode` なし・sonnet・`--resume` | 14.8 |
| `--safe-mode`・sonnet | 5.6 / 6.3 / 5.0 / 7.6 |
| `--safe-mode`・haiku | 4.7 / 5.2 |

起動に約 3 秒かかり（`duration_api_ms` は 1.7〜4.6 秒）、モデルを軽くしても縮まらない。
