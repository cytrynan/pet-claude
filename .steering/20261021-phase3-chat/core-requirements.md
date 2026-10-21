---
type: Steering
description: pet-core::chat（チャットの純粋ロジック）の要求。テスト作成係と実装係の唯一の原本（#8）
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-21T22:35:00+09:00 }
---

# 要求: `pet-core::chat`（チャットの子プロセスまわりの純粋な関数）

デスクトップのペット（pet-app）は、バーのチャットに打たれた文を `claude -p`（Claude Code の CLI）の子プロセスに渡し、
答えの JSON を読んで吹き出しとチャット欄に出す。このモジュールは、その**引数・環境変数・出力の解析・表示用の整形・実行ファイルの探索**を持つ。
**OS にもファイルにもネットワークにも触らない純粋な関数**にする（ファイルの有無は引数の関数で受け取る）。

ファイル: `crates/pet-core/src/chat.rs`（`pub mod chat;` を `lib.rs` に足す）、テストは `crates/pet-core/tests/chat.rs`。
依存は既存のもの（`serde`・`serde_json`）だけ。既存の `pet_core::hook::sanitize_line(s, max_chars)` は使ってよい
（制御文字を空白に、空白の並びを半角 1 つに、前後の空白を落とし、`max_chars` 字を超えたら `max_chars-1` 字＋`…`）。

## 用途と脅威（なぜ厳しくするか）

- 子プロセスには K の Claude のサブスクか API キーが付く。**環境の組み立てを間違えると、K が気づかないまま API 課金になる**（`claude -p` は環境に `ANTHROPIC_API_KEY` があれば必ずそれを使う）
- 答えの文・エラーの文は外（モデル・CLI）から来るので、**吹き出しに出す前に必ず整形・検査する**（N3: 吹き出しの動的テキストは 1〜140 字・単一行・コード/URL/パス/秘密らしい文字列は出さない）
- API キーは画面にもログにも出さない（N4）

## 識別子契約（この名前・型で作る）

```rust
pub const MODEL: &str = "sonnet";
pub const TIMEOUT_SECS: u64 = 120;
pub const BUBBLE_MAX_CHARS: usize = 140;
pub const PANEL_MAX_CHARS: usize = 4000;
pub const CREDENTIAL_TARGET: &str = "pet-claude/anthropic-api-key";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode { Subscription, ApiKey }
impl AuthMode {
    pub fn from_state(v: Option<&str>) -> AuthMode;
    pub fn as_str(&self) -> &'static str;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatError { MissingKey }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatOutcome {
    Answer { text: String, session_id: Option<String> },
    Failed { reason: String, session_id: Option<String> },
}

pub fn is_session_id(s: &str) -> bool;
pub fn child_args(system_prompt_file: &str, resume: Option<&str>) -> Vec<String>;
pub fn child_env(mode: AuthMode, parent: &[(String, String)], api_key: Option<&str>)
    -> Result<Vec<(String, String)>, ChatError>;
pub fn parse_output(exit_code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> ChatOutcome;
pub fn bubble_text(s: &str) -> String;
pub fn panel_text(s: &str) -> String;
pub fn find_claude(path_dirs: &[std::path::PathBuf], home: Option<&std::path::Path>,
    exists: &dyn Fn(&std::path::Path) -> bool) -> Option<std::path::PathBuf>;
pub fn auth_status_label(stdout: &[u8]) -> String;
```

## 1. `AuthMode`

- `from_state(Some("api_key"))` → `ApiKey`。それ以外（`None`・`"subscription"`・知らない値・大文字違いの `"API_KEY"`・空文字）はすべて `Subscription`（既定はサブスク）
- `as_str()`: `Subscription` → `"subscription"`、`ApiKey` → `"api_key"`。`from_state(Some(m.as_str())) == m` が両方で成り立つ

## 2. `is_session_id`

- Claude Code のセッション id は UUID の文字列。`8-4-4-4-12` 桁の 16 進（`0-9a-f` と `A-F`）をハイフンで区切った**ちょうど 36 字**だけ `true`
- 前後の空白・波括弧・ハイフンの位置違い・16 進でない文字・長さ違い・空文字は `false`

## 3. `child_args`

返す並び（この順で完全一致）:

```
-p
--output-format  json
--safe-mode
--model          sonnet            ← MODEL
--tools          ""                ← 空文字の要素 1 つ（組み込みツールを全部外す）
--disallowedTools mcp__*
--strict-mcp-config
--settings       {"disableAllHooks":true}
--disable-slash-commands
--permission-prompts none
--max-turns      1
--system-prompt-file <system_prompt_file>
[--resume <id>]                    ← resume が Some で is_session_id を満たす時だけ末尾に 2 要素
```

- `resume` が `Some` でも `is_session_id` を満たさなければ `--resume` を付けない（新しい会話として起こす。外から来た値を引数に混ぜない）
- 本文（ユーザーの文）は引数に入れない（標準入力で渡す。この関数は本文を受け取らない）
- `system_prompt_file` はそのまま 1 要素として入れる（空白を含むパスも分割しない）

## 4. `child_env`

`parent` は親プロセスの環境変数（名前, 値）の並び。返すのは子に渡す並び。

- 次の名前は**必ず取り除く**（Windows の環境変数名は大文字小文字を区別しないので、**ASCII の大文字小文字を無視して**一致させる。例 `anthropic_api_key` も消す）:
  `ANTHROPIC_API_KEY`・`ANTHROPIC_AUTH_TOKEN`・`CLAUDE_CODE_USE_BEDROCK`・`CLAUDE_CODE_USE_VERTEX`・`CLAUDE_CODE_USE_FOUNDRY`・`ANTHROPIC_PROFILE`・`CLAUDECODE`・`CLAUDE_CODE_ENTRYPOINT`
- 名前が似ているだけのもの（`ANTHROPIC_API_KEY_OLD`・`MY_ANTHROPIC_API_KEY`・`CLAUDE_CODE_OAUTH_TOKEN`・`PATH`）は残す。残すものは**元の順と値のまま**
- `Subscription`: 上を取り除いたものを返す。`api_key` は**無視する**（`Some` でも子に入れない）
- `ApiKey`: 上を取り除いたうえで、末尾に `("ANTHROPIC_API_KEY", key)` を 1 つ足す。`key` は前後の空白を落とした値。`api_key` が `None`・空・空白だけなら `Err(ChatError::MissingKey)`
- `ApiKey` で返す並びに `ANTHROPIC_API_KEY`（大文字小文字無視）は**ちょうど 1 つ**

## 5. `parse_output`

`claude -p --output-format json` の結果を読む。標準出力は UTF-8 の JSON オブジェクト 1 つ（前後に空白・改行があってよい）。

- 標準出力（前後の空白を落とす）が JSON オブジェクトで、`"type": "result"` を持つとき:
  - `session_id` は文字列で `is_session_id` を満たす時だけ `Some`（成功でも失敗でも同じ）
  - `"is_error": true` → `Failed`。`reason` は `result`（文字列なら）を §6 の `bubble_text` に通したもの。`result` が無い・文字列でない・空なら `"Claude Code がエラーを返しました"`
  - `"is_error"` が `false` か無い、かつ `result` が空でない文字列 → `Answer { text: result の生の文字列, .. }`（整形しない。表示の時に §6・§7 を通す）
  - それ以外（`result` が無い・空・文字列でない）→ `Failed { reason: "答えが空でした", .. }`
  - 終了コードが 0 以外でも、JSON が読めたら上の規則に従う（実測: 上限・鍵違いは終了コード 1 で `is_error: true` の JSON を出す）
- 標準出力が JSON として読めない・オブジェクトでない・`type` が `"result"` でない → `Failed { session_id: None, reason }`:
  - 標準エラーを UTF-8 として読み（不正なバイトは置き換え）、最初の「空白以外を含む行」を `bubble_text` に通したものを `reason` に
  - 標準エラーが空（空白だけ）なら `"Claude Code が答えませんでした（終了コード N）"`。`exit_code` が `None` なら `"Claude Code が答えませんでした（終了コード なし）"`

## 6. `bubble_text`（吹き出しに出す 1 行）

- 結果は**必ず 1〜140 字（`chars().count()`）の単一行**（`\n`・`\r`・その他の制御文字を含まない）
- 手順: `sanitize_line(s, BUBBLE_MAX_CHARS)` で 1 行にする。空になったら `"（答えが空でした）"`
- 次のどれかを含む文は、吹き出しには出さず `"（答えはチャット欄に出しました）"` を返す（N3。中身はチャット欄 §7 で見られる）。判定は sanitize 後の文で行う:
  - URL: `://` を含む、または `www.` を含む
  - パス: `英字 + ":\"` か `英字 + ":/"`（例 `C:\`・`d:/`）、`\\`（UNC の先頭）、`~/`、`/Users/`・`/home/`・`/etc/` を含む
  - コード: バッククォート（`` ` ``）を含む
  - 秘密らしい文字列: `sk-` で始まる語（直前が英数字でない `sk-`）、または英数字・`_`・`-` だけが **32 字以上**続く並び
- 普通の日本語・英語の文（句読点・絵文字・`%`・`:` 単体・`/` 単体を含む）はそのまま（140 字を超えたら sanitize_line の切り詰め）

## 7. `panel_text`（チャット欄に出す全文）

- 改行は残す: `\r\n` と単独の `\r` は `\n` に。`\n` 以外の制御文字（`\t` を含む）は半角空白に
- 前後の空白・改行は落とす
- `PANEL_MAX_CHARS` 字を超えたら先頭 `PANEL_MAX_CHARS - 1` 字＋`…`（ちょうど `PANEL_MAX_CHARS` 字）
- 空になったら `"（答えが空でした）"`

## 8. `find_claude`

`path_dirs` は PATH を分けたフォルダの並び（先頭が優先）。`exists(p)` はそのファイルがあるか（テストでは偽の関数を渡す）。

1. `path_dirs` を先頭から見て、各フォルダ `d` で:
   - `d\claude.exe` があればそれを返す
   - 無ければ、`d\claude.cmd` があり、かつ `d\node_modules\@anthropic-ai\claude-code\bin\claude.exe` があればそれを返す（npm 版。`.cmd` 自体は返さない）
2. どのフォルダにも無ければ、`home` が `Some(h)` で `h\.local\bin\claude.exe` があればそれを返す（ネイティブ版）
3. 無ければ `None`
- パスは `Path::join` で組む（`d.join("claude.exe")`、`d.join("node_modules").join("@anthropic-ai").join("claude-code").join("bin").join("claude.exe")`、`h.join(".local").join("bin").join("claude.exe")`）
- 空のパス（`""`）のフォルダは飛ばす（カレントフォルダの実行ファイルを拾わない）
- `claude.cmd` だけある（隣の exe が無い）フォルダは飛ばして次のフォルダを見る

## 9. `auth_status_label`（設定画面の「いまの接続」）

`claude auth status`（既定で JSON）の標準出力から、画面に出す短い文を作る。**メールアドレス・組織名は出さない**。

- JSON で `loggedIn` が `false` → `"ログインしていません"`
- `loggedIn` が `true` かつ `authMethod` が `"claude.ai"` → `"サブスク（<subscriptionType>）"`。`subscriptionType` が無い・文字列でないなら `"サブスク"`。`subscriptionType` は英数字だけの時だけ使う（それ以外は `"サブスク"`）
- `authMethod` が `"api_key"` → `"API キー"`
- それ以外の文字列の `authMethod` → `"その他の接続"`
- JSON として読めない・`loggedIn` が無い → `"確かめられませんでした"`

## 追補: 質問票（questions.md）への裁定（司令塔 2026-10-21）

- **Q1 → 代案を採る（安全側）**: §6 の検査（URL・パス・コード・秘密）は、**切り詰める前の** 1 行（`sanitize_line(s, usize::MAX)` 相当）で行う。検査に通ったときだけ `BUBBLE_MAX_CHARS` に切り詰めて返す。理由: 境界をまたぐ鍵の先頭が吹き出しに出るのを防ぐ（N3・N4）
- **Q2**: 「英数字」「英字」は ASCII に限る（採った解釈どおり）
- **Q3 → 大文字小文字を無視する（安全側）**: `sk-`・`www.`・`://` は ASCII の大文字小文字を無視して見つける（`SK-…`・`WWW.example.com` も隠す）
- Q4〜Q11: テスト作成係の採った解釈どおり（Q6 は「その他の接続」、Q7 は空文字なら `"サブスク"`、Q8 は `null`・真偽値でない `loggedIn` は「確かめられませんでした」、Q9 は前後を落としてから数える）

## 追補 2: 検証係（Codex）の指摘の回収（司令塔 2026-10-21）

指摘 5（チャット欄に秘密が出る）・6（失敗の理由が吹き出しで置き換わると、どこにも残らない）・2（本文の大きさに上限が無い）への対応。**ここに書いたものが §5・§7 の該当箇所より優先する。**

### 識別子契約に足すもの

```rust
pub const MAX_INPUT_CHARS: usize = 2000;
pub const BUBBLE_HIDDEN: &str = "（答えはチャット欄に出しました）";
pub const FAILURE_MAX_CHARS: usize = 200;
pub fn mask_secrets(s: &str) -> String;
pub fn input_text(s: &str) -> Option<String>;
```

### A. `BUBBLE_HIDDEN`

§6 で検査に掛かったときに返す文はこの定数と同じ（値は変えない）。

### B. `mask_secrets`（秘密らしい文字列を伏せる）

- §6 の「秘密らしい文字列」と同じ判定（`sk-` で始まる語＝直前が ASCII 英数字でない `sk-`。大文字小文字を無視／ASCII 英数字・`_`・`-` だけが 32 字以上続く並び）に当たる部分を、それぞれ `••••`（U+2022 を 4 つ）に置き換える
- `sk-` で始まる語の範囲: `sk-` から、ASCII 英数字・`_`・`-` が続く限り（その後ろの文字は残す）。例 `鍵は sk-ant-abc123 です` → `鍵は •••• です`
- 32 字以上の並びは、並び全体を 1 つの `••••` に置き換える
- それ以外の文字（改行・URL・パス・日本語）はそのまま。秘密が無ければ入力と同じ文字列

### C. `panel_text` の変更（§7 に足す）

§7 の手順の**最初に** `mask_secrets` を通す（その後に改行の正規化・制御文字・前後の空白・切り詰め）。チャット欄にも秘密らしい文字列は出さない（N4）。

### D. `parse_output` の失敗の理由（§5 を変える）

- `Failed.reason` は `bubble_text` を通さない。代わりに **1 行の失敗の文**: `mask_secrets` → `sanitize_line(_, FAILURE_MAX_CHARS)`。空になったら従来の既定の文（`"Claude Code がエラーを返しました"` など §5 の各場合の文）
- §5 の他の規則（どの場合に Failed か・既定の文・session_id）は変えない
- 吹き出しに出すときはアプリ側が `bubble_text` を通す（パスを含む理由は吹き出しでは隠れ、チャット欄では見える。秘密はどちらにも出ない）

### E. `input_text`（送る本文）

- 前後の空白を落とす。空なら `None`
- `MAX_INPUT_CHARS` 字（`chars().count()`）を超えたら `None`（切り詰めずに断る）
- それ以外はその文字列（内部の改行などはそのまま）
- 追補 2 の質問への裁定: Q12 前後の空白を落としてから 2000 字を数える（採った解釈どおり）／Q13 `Answer.text` は伏せない（表示の時に panel_text と bubble_text が伏せる・隠す）／Q14 直前が `_`・`-` の `sk-` は伏せる（安全側。直前が ASCII 英数字の時だけ残す）
