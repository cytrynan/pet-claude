---
name: codex-interop
description: Claude Code の個人設定（~/.claude の CLAUDE.md・skills・agents・hooks・知見フォルダ）を OpenAI Codex からも読み書きできるようにする初期設定と、その後の同期。正本は ~/.claude のまま、Codex 側にはジャンクション（シンボリックリンク）と同梱スクリプトの生成物だけを置く。次のような依頼で使う:「Codex にも Claude の設定・スキル・知見を使わせたい」「Codex と規約を共有して」「~/.claude を Codex から読み書きできるようにして」「Codex 用の AGENTS.md を作って」、Codex 側の生成物が古い・壊れたとき。逆に、Codex に差分を検証させる作業（codex-verify）、リポジトリ単位の AGENTS.md の作成、Codex から Claude への一方向の取り込みには使わない。
---

# codex-interop — Claude Code の設定を Codex と共有する

## 考え方

- **正本は `~/.claude` の1か所**。Codex 用に書き直したコピーを手で持たない（複製は必ず陳腐化する）。
- 形式が同じものはリンク、違うものは同梱スクリプトで生成する。生成物には目印を入れ、目印の無い（人が書いた）ファイルは上書きしない。
- Claude 固有の語（Edit/Write→apply_patch、Agent→spawn_agent、モデル名など）は、Codex 向け `AGENTS.md` の**前置きの読み替え表**で吸収する。

| Claude（正本） | Codex 側 | 方式 |
|---|---|---|
| `~/.claude/CLAUDE.md` | `~/.codex/AGENTS.md` | 生成: 前置き `~/.claude/codex/AGENTS.header.md` ＋ CLAUDE.md 本文の転記 |
| `~/.claude/skills/<name>/` | `~/.agents/skills/<name>` | ジャンクション（Windows）／シンボリックリンク。除外は `sync-config.json` |
| `~/.claude/agents/*.md` | `~/.codex/agents/<name>.toml` | 生成（`model` → `model_reasoning_effort` の対応表、`disallowedTools`/`tools` → 指示文） |
| `settings.json` の hooks | `~/.codex/hooks.json` | 生成（Stop は `codex-hook-adapter.mjs` で包み、Codex の会話記録を Claude 形式に直して同じスクリプトへ渡す） |
| 知見フォルダ | `config.toml` の `sandbox_workspace_write.writable_roots` | 手で1回 |

公式の取り込み（Codex の `/import`・デスクトップの Settings > Import）は一回きりのコピーで、本文中の「Claude」「CLAUDE.md」を
機械置換するため `~/.claude/...` のパス参照が壊れうる。継続運用にはこのスキルの同期を使う。

## 初期設定の手順

1. 同梱スクリプトを置く: `scripts/sync-codex.mjs` と `scripts/codex-hook-adapter.mjs` を `~/.claude/scripts/` へコピーする（同期スクリプトはアダプタをこの場所で参照する）。
2. 設定を置く: `templates/AGENTS.header.md` と `templates/sync-config.json` を `~/.claude/codex/` へコピーし、
   前置きの「正本のフォルダ構成」「このPCの既知の制約」と、除外するスキル（Claude 専用のもの）を自分の環境に合わせて直す。
   ホワイトリスト方式の `.gitignore` で `~/.claude` を管理しているなら `!/codex/` を足す。
3. 試しに差分を見る: `node ~/.claude/scripts/sync-codex.mjs --check`（書かない。差分があれば exit 1）。
   既存の `~/.codex/AGENTS.md`・`hooks.json`・同名の `agents/*.toml` があれば skipped と出て上書きされない。中身を確かめて退避してから再実行する。
4. 同期する: `node ~/.claude/scripts/sync-codex.mjs`。
5. `~/.codex/config.toml` に追記する（トップレベルのキーは最初の `[table]` より前に書く）:
   ```toml
   project_doc_fallback_filenames = ["CLAUDE.md"]   # AGENTS.md が無いリポジトリでは CLAUDE.md を読む
   [sandbox_workspace_write]
   writable_roots = ['<ホーム>\.claude']            # 絶対パス。サブフォルダ単位にしない（下の注意）
   ```
6. 自動同期: `~/.claude/settings.json` の SessionStart に `node "<ホーム>/.claude/scripts/sync-codex.mjs" --quiet` を足す（成功時は無出力）。
   次回の同期で Codex の hooks.json にも入り、Codex 側の起動でも同期される。
7. Codex のフックを信頼する: CLI で `codex` を起動し `/hooks` で生成されたフックを信頼する（初回と hooks.json が変わるたび）。
8. 確認: `codex exec --skip-git-repo-check -s read-only --ephemeral "AGENTS.md の先頭見出し・使えるカスタムエージェント・ユーザースキルの一覧を答えて"`。

## 注意（実測に基づく）

- **writable_roots は `~/.claude` 単位で登録する**。Codex は書き込みルート直下の `.git`・`.agents`・`.codex` を読み取り専用に保護するため、
  `~/.claude/dev` のような入れ子リポジトリをルートにすると、その `.git` が守られてコミットできなくなる。
- スキルの編集はリンク側（`~/.agents/skills/...`）でなく正本のパス（`~/.claude/skills/...`）で行う。リンク側はサンドボックスの書き込みルート外になる。
- Codex のカスタムエージェントはサンドボックスを個別に指定できない。Claude の `disallowedTools` は指示文に変換されるだけなので、
  機構で書き込みを塞ぐには親セッションを read-only で起動する。
- Stop フックは Codex の rollout（会話記録）から `FileChange`（成功した編集）と `CommandExecution` を拾う。`--ephemeral` では記録が残らないので何も検知しない。
- ジャンクションの掃除はリンク自体だけを消す。`rm -r` 系でリンクを消すとリンク先（正本）の中身まで消えることがあるので、手作業でも使わない。
- Windows の elevated サンドボックスで、デスクトップアプリが書いた設定との組み合わせにより apply_patch もシェルも失敗する例がある
  （サンドボックス準備ログに os error 32）。原因は同梱 `node_repl.exe` が使用中なこと（ユーザー設定の MCP で Codex 自身が起動したものか、起動中のデスクトップアプリのもの）。
  CLI は `--ignore-user-config -c windows.sandbox="elevated"` で回避でき、デスクトップアプリ起動中は Codex を起動するプロセスの `LOCALAPPDATA` を空フォルダへ向けると通る。
  その場合 config.toml の writable_roots・model・model_reasoning_effort などは `-c` で渡し直す必要がある。検証係としての完成形はスキル `codex-verify` の起動形。

## 同梱物

- `scripts/sync-codex.mjs` — 同期本体。引数: なし＝同期／`--check`＝差分確認（exit 1）／`--quiet`＝成功時無出力・失敗しても exit 0／`--force`＝人が書いた AGENTS.md も上書き。
  環境変数 `CLAUDE_CONFIG_DIR`・`CODEX_HOME`・`CODEX_SYNC_AGENTS_SKILLS_DIR` で入出力先を差し替えられる（試験の隔離用）。
- `scripts/codex-hook-adapter.mjs` — Codex の Stop 入力を Claude 用 Stop フックが読める形に直して実行する。常に exit 0。
- `templates/AGENTS.header.md` — Codex 向け前置きのひな形（読み替え表・役割分担・フック）。
- `templates/sync-config.json` — スキルの除外と、モデル名 → 推論強度の対応表。
