# Codex 向けの前置き（Claude Code 用の規約を Codex で使うための読み替え）

このPCの作業規約・知見の正本は Claude Code 用の `~/.claude/` にある（`~` = ホームディレクトリ）。
このファイルの後半に `~/.claude/CLAUDE.md` の本文をそのまま転記してある。**以下の読み替えをしたうえで、後半の規約に Claude と同じように従うこと。**

## 1. 正本と生成物

- 正本は `~/.claude/`（例: 技術知見 `dev/`・業務 `work/`・個人 `home/`・小さな事実 `memory/`・スキル `skills/`・サブエージェント `agents/`・フック実体 `scripts/`。
  自分の構成に合わせて書き換える）。地図があれば `~/.claude/MAP.md`。
- Codex も `~/.claude/` 配下を読み書きしてよい（`~/.codex/config.toml` の `sandbox_workspace_write.writable_roots` に登録済み）。
  書くときの規約（置き場所・`log.md` への1行追記・秘密を書かない・dev push・テンプレ還流・報告）は Claude と同じ。
- ただし MAP.md の「自動生成物」（`projects/`・`sessions/`・`.credentials.json`・`settings.local.json` など）には触れない。
- 次の4つは**生成物なので直接編集しない**（次回の同期で上書きされる）。直すときは正本を直してから `node ~/.claude/scripts/sync-codex.mjs` を実行する。

| Codex 側（生成物） | 正本 |
|---|---|
| `~/.codex/AGENTS.md`（このファイル） | 前半 `~/.claude/codex/AGENTS.header.md`、後半 `~/.claude/CLAUDE.md` |
| `~/.agents/skills/<name>` | `~/.claude/skills/<name>` へのジャンクション。編集は `~/.claude/skills/<name>/` のパスで行う |
| `~/.codex/agents/<name>.toml` | `~/.claude/agents/<name>.md` |
| `~/.codex/hooks.json` | `~/.claude/settings.json` の `hooks` |

## 2. 用語・ツール名の読み替え

後半の規約・スキル・dev/ の知見は Claude Code の語で書かれている。Codex では次のように読む。

| Claude の語 | Codex での読み替え |
|---|---|
| グローバル `CLAUDE.md` | この AGENTS.md |
| リポジトリの `CLAUDE.md` | AGENTS.md が無いリポジトリでは自動で読まれる（`project_doc_fallback_filenames`）。中の `@path` 形式の import は展開されないので、必要なら自分で読む |
| Read / Grep / Glob | ファイルの読み取り・検索 |
| Write / Edit / MultiEdit | `apply_patch` |
| Bash / PowerShell ツール | シェル実行 |
| Agent ツール（`subagent_type`） | `spawn_agent`。`readonly-reviewer`・`design-director` は同名のカスタムエージェントとして使える |
| Skill ツール・`/スキル名` | `$スキル名` で明示呼び出し（説明文に合えば自動でも使われる） |
| AskUserQuestion | ユーザーへの通常の質問 |
| タスクリスト（update_plan / TaskCreate） | `update_plan` |
| WebSearch / WebFetch | Web 検索 |
| `mcp__claude-in-chrome__*`・Claude の内蔵ブラウザ | Codex の browser / Chrome プラグイン |
| `mcp__ccd_*`（Claude デスクトップアプリ専用）・statusline・`/weekly` | 該当なし（無視する） |
| モデル名 Fable / Opus / Sonnet / Haiku | Codex では選べない。下の「3. 役割分担」に従う |
| auto memory（`~/.claude/projects/<dir>/memory/`） | 読むだけ。Codex 自身の記憶は Codex の memories 機能 |
| コミット末尾の `Co-Authored-By: Claude …` | 付けない |

## 3. 役割分担（後半「モデル別役割分担ワークフロー」の Codex 版）

- この環境での Codex の既定の役割は**標準フローの検証係**（Claude Code の司令塔からスキル `codex-verify` 経由の `codex exec` で呼ばれる。リポジトリは書き込み不可のまま、テスト・lint は実行できる）。
  その依頼では依頼文が優先し、ファイルを変更しない。
- ユーザーが Codex を直接使うときは Codex が本体（司令塔）。後半の規約（記録・issue 同期・dev push・テンプレ還流・パスワード非記録・報告）を同じように守る。
  - 調査・実装の分担には `spawn_agent`（組み込みの explorer / worker、カスタムエージェント）を使ってよい。
  - **検証は別ベンダーに任せる原則は維持する**。Codex が本体として行った作業は、Claude Code の `readonly-reviewer` での検証をユーザーに提案する。
    自分の作業を自分で「検証済み」と報告しない。
  - スキル `codex-verify` は Claude → Codex 方向の専用なので、Codex からは使わない（同期の対象外）。

## 4. フック

- `~/.claude/settings.json` のフックは `~/.codex/hooks.json` に変換済み。SessionStart のリマインド注入・dev の pull・この同期、
  Stop の log.md 追記漏れ検知と記録の催促が Codex でも動く（Stop は `~/.claude/scripts/codex-hook-adapter.mjs` が Codex の会話記録を Claude 形式に直して同じスクリプトへ渡す）。
- 差し戻されたら、指示どおり記録してから終える。フックが動いていない様子なら、CLI の `/hooks` で信頼済みかを確認するようユーザーに伝える。

## 5. このPCの既知の制約（あれば書く）

- 例: Windows の elevated サンドボックスで、デスクトップアプリが書いたユーザー設定との組み合わせにより書き込みが失敗する環境では、
  CLI を `--ignore-user-config -c windows.sandbox="elevated"` で起動すると回避できることがある。書き込みに失敗したら別の経路で無理に書かず、ユーザーに報告する。
