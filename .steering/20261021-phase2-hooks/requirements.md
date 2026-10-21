---
type: Steering
description: Phase 2 の中心ロジック（pet-core::hook と pet-core::ipc）の要求。テスト作成係と実装係の唯一の原本
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-21T10:40:00+09:00 }
---

# 要求: Claude Code の hook 入力の解析と、アプリとの通信の型（`pet-core::hook`・`pet-core::ipc`）

Claude Code は出来事のたびに hook のプログラム（`pet-hook.exe`）を起動し、標準入力に JSON を 1 つ渡す。
`pet-hook.exe` はそれを解析してペットのアプリ（`pet-app.exe`）へ loopback の HTTP で送る。承認待ち（`PermissionRequest`）だけは、アプリの答えを待って、許可・拒否を標準出力に返す。
この 2 つのモジュールは、その解析・要約・出力の組み立てと、HTTP の最小限の読み書きを持つ。**OS にもネットワークにも触らない純粋な関数**にする。

一次資料（2026-10-21 に確認）: https://code.claude.com/docs/en/hooks の「PermissionRequest」「Notification」「UserPromptSubmit」「Stop」「StopFailure」「PreToolUse」の節。

## 識別子契約（この名前と型を変えない）

ファイル: `crates/pet-core/src/hook.rs` と `crates/pet-core/src/ipc.rs`（`lib.rs` に `pub mod hook; pub mod ipc;` を足す）。既存の `pet_core::behavior::AgentEvent` を使う。依存は既存の `serde`・`serde_json` だけ（`Cargo.toml` を変えない）。

**テストは `crates/pet-core/tests/hook.rs` と `crates/pet-core/tests/ipc.rs`（統合テスト）に置き、公開 API だけを使う。`src/` の中にテストを書かない。**

### hook.rs

```rust
use crate::behavior::AgentEvent;

/// 標準入力として受け付ける最大のバイト数。これを超えたら解析しない。
pub const MAX_INPUT_BYTES: usize = 65_536;
/// 承認の吹き出しに出す要約の最大文字数（Unicode のスカラー値で数える）。
pub const SUMMARY_MAX_CHARS: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationKind {
    PermissionPrompt,      // "permission_prompt"
    IdlePrompt,            // "idle_prompt"
    ElicitationDialog,     // "elicitation_dialog"
    Other(String),         // それ以外の値（値はそのまま）
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookInput {
    Prompt { session: String },                                    // UserPromptSubmit
    ToolStart { session: String, tool: String },                   // PreToolUse
    PermissionRequest { session: String, tool: String, summary: String },
    Notification { session: String, kind: NotificationKind },
    Stop { session: String },
    StopFailure { session: String, error: String },
    Other { session: String, event: String },                     // 上以外の hook_event_name
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookError {
    TooLarge,                     // 入力が MAX_INPUT_BYTES を超えた
    InvalidJson,                  // JSON として読めない、またはオブジェクトでない
    MissingField(&'static str),   // 必須の項目が無い・文字列でない（項目名を入れる）
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny { message: String },
    /// 決めない（Claude Code のいつもの承認の流れに戻す）
    Ask,
}

pub fn parse(input: &[u8]) -> Result<HookInput, HookError>;
pub fn summarize(tool: &str, tool_input: &serde_json::Value) -> String;
pub fn sanitize_line(s: &str, max_chars: usize) -> String;
pub fn to_agent_event(input: &HookInput) -> Option<AgentEvent>;
pub fn permission_output(decision: &Decision) -> String;
```

### ipc.rs

```rust
use serde::{Deserialize, Serialize};

/// アプリが起動時に書く発見ファイルの中身（JSON）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Discovery {
    pub endpoint: String,   // 例 "127.0.0.1:53211"
    pub token: String,
    pub pid: u32,
}

/// hook からアプリへ送る出来事（HTTP の本文の JSON）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireEvent {
    pub kind: String,       // "prompt" | "tool_start" | "permission" | "notification" | "stop" | "stop_failure" | "other"
    pub session: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>, // notification の種類（"permission_prompt" など）や stop_failure の error
}

/// 承認の状態（GET /permission/<id> の応答の JSON）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState { Pending, Allow, Deny, Ask }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,              // "GET" / "POST"（大文字のまま）
    pub path: String,                // 例 "/event"、"/permission/abc"
    pub authorization: Option<String>, // Authorization ヘッダの値（前後の空白を除く）
    pub origin: Option<String>,      // Origin ヘッダの値
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Incomplete,     // まだ全部届いていない（読み続ける）
    Malformed,      // HTTP として壊れている
    TooLarge,       // 下の 7 節の規則で上限を超える
}

pub const MAX_REQUEST_BYTES: usize = 131_072;

impl From<&crate::hook::HookInput> for WireEvent { /* 下の対応表 */ }

pub fn parse_request(buf: &[u8]) -> Result<Request, ParseError>;
pub fn format_response(status: u16, body: &str) -> Vec<u8>;
pub fn format_request(method: &str, path: &str, token: &str, body: &str) -> Vec<u8>;
pub fn is_authorized(req: &Request, token: &str) -> bool;
```

## 振る舞い: hook.rs

### 1. `parse`

- `input.len() > MAX_INPUT_BYTES` なら `Err(TooLarge)`（ちょうど等しいときは解析する）
- UTF-8 の JSON オブジェクトとして読めなければ `Err(InvalidJson)`（配列・文字列・数値の JSON も `InvalidJson`）
- **検査の順序は固定**: `TooLarge` → `InvalidJson` → `hook_event_name` → `session_id` → イベント別の必須項目（下の表）。最初に見つかった誤りだけを返す
- 「無い」とは、項目が無い・`null`・文字列でない（数値・真偽・配列・オブジェクト）のどれか。**空文字 `""` は「ある」**（そのまま使う）
- `hook_event_name` が無ければ `Err(MissingField("hook_event_name"))`、`session_id` が無ければ `Err(MissingField("session_id"))`
- 入力の先頭の UTF-8 BOM は `InvalidJson`。JSON の前後の空白・改行は許す
- `hook_event_name` ごとの対応:

| hook_event_name | 結果 | 必須の項目（無ければ MissingField） |
|---|---|---|
| `UserPromptSubmit` | `Prompt` | — |
| `PreToolUse` | `ToolStart { tool: tool_name }` | `tool_name` |
| `PermissionRequest` | `PermissionRequest { tool: tool_name, summary: summarize(tool_name, tool_input) }` | `tool_name`（`tool_input` が無い・オブジェクトでないときは `summarize` に `Null` を渡す） |
| `Notification` | `Notification { kind }`。`notification_type` の値で上の `NotificationKind` に対応づける。`notification_type` が無い（上の定義）ときは `Other("")`。空文字のときも `Other("")` | — |
| `Stop` | `Stop` | — |
| `StopFailure` | `StopFailure { error }`。`error` が無い（上の定義）ときは `"unknown"`。空文字はそのまま `""` | — |
| それ以外 | `Other { event: hook_event_name }` | — |

- 上の表に無い項目（`prompt`・`transcript_path`・`last_assistant_message`・`message`・`title`・`error_details` など）は読まない。**プロンプトや応答の本文を `HookInput` に入れない**
- 検査の形（テストはこの手順で書く）: 各イベントの入力の、上の表で使わない項目（`prompt`・`last_assistant_message`・`transcript_path`・`message`・`title`・`error_details`・`tool_input` の `content`・`old_string`・`new_string`、`summarize` が取り出さない `description` など）に、別々の目印の文字列を入れる。`format!("{:?}", 結果)` と `serde_json::to_string(&WireEvent::from(&結果))` のどちらにも目印が含まれないこと。例外は `summarize` が取り出す項目（表 2）の値で、`summary` に入ってよい
- `summary` 以外の文字列（`session`・`tool`・`error`・`Other` の中身）は加工せずそのまま渡す（長さの上限も付けない。表示する側で `sanitize_line` を通す）

### 2. `summarize`（承認の吹き出しに出す 1 行の要約）

| tool | 取り出す値（文字列のときだけ） |
|---|---|
| `Bash`・`PowerShell` | `command` |
| `Read`・`Write`・`Edit`・`MultiEdit` | `file_path` |
| `NotebookEdit` | `notebook_path` |
| `WebFetch` | `url` |
| `WebSearch` | `query` |
| `Grep`・`Glob` | `pattern` |
| `Agent`・`Task` | `description` |
| `mcp__` で始まる名前 | 取り出さない（空文字） |
| それ以外 | 取り出さない（空文字） |

- 値が無い・文字列でないときは空文字
- 取り出した値は `sanitize_line(value, SUMMARY_MAX_CHARS)` を通して返す

### 3. `sanitize_line`

- Unicode の制御文字（`char::is_control`）を空白に置き換える（改行・タブも含む）
- 連続する空白（`char::is_whitespace`）を 1 つの半角空白にまとめ、前後の空白を除く
- その結果の文字数（`chars().count()`＝Unicode のスカラー値の数）が `max_chars` を超えたら、先頭 `max_chars - 1` 文字＋`…`（U+2026）にする（結果の文字数はちょうど `max_chars`）。**切り詰めた後はもう一度 trim しない**（末尾が空白のままでもよい。例 `sanitize_line("a bcd", 3)` は `"a …"`）。`max_chars` 以下ならそのまま
- `max_chars == 0` のときは空文字。`max_chars == 1` で 2 文字以上のときは `"…"`
- 置き換え・まとめの順序は「制御文字を空白へ → 空白の連続を 1 つに → 前後の空白を除く → 切り詰め」。ゼロ幅文字（U+200B・U+200D・U+FEFF）は制御文字でも空白でもないので残り、1 文字と数える。絵文字の合字が途中で切れてもよい

### 4. `to_agent_event`

| HookInput | AgentEvent |
|---|---|
| `Prompt` | `Prompt { session }` |
| `ToolStart` | `ToolStart { session }` |
| `PermissionRequest` | `NeedsInput { session }` |
| `Notification { kind: PermissionPrompt }`・`Notification { kind: ElicitationDialog }` | `NeedsInput { session }` |
| `Notification` のそれ以外（`IdlePrompt`・`Other`） | `None` |
| `Stop` | `Done { session }` |
| `StopFailure` | `Error { session }` |
| `Other` | `None` |

### 5. `permission_output`（hook が標準出力に書く JSON）

- `Allow` → `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}`
- `Deny { message }` → `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny","message":<message>}}}`（`message` は JSON の文字列として正しくエスケープする）
- `Ask` → `{}`
- 改行を含まない 1 行。**キーの順序は保証しない**（テストは `serde_json::from_str` で読み直して値を比べる。文字列の完全一致は `Ask` の `{}` だけ）

## 振る舞い: ipc.rs

### 6. `WireEvent` の対応（`From<&HookInput>`）

| HookInput | kind | tool | summary | detail |
|---|---|---|---|---|
| `Prompt` | `"prompt"` | None | None | None |
| `ToolStart` | `"tool_start"` | Some(tool) | None | None |
| `PermissionRequest` | `"permission"` | Some(tool) | Some(summary) | None |
| `Notification` | `"notification"` | None | None | Some(種類の文字列: `"permission_prompt"` / `"idle_prompt"` / `"elicitation_dialog"` / Other の中身) |
| `Stop` | `"stop"` | None | None | None |
| `StopFailure` | `"stop_failure"` | None | None | Some(error) |
| `Other` | `"other"` | None | None | Some(event) |

`session` はそのまま。JSON にするとき `None` の項目は出さない。読むときは無くてよい。

### 7. `parse_request`（アプリ側が受けた HTTP/1.1 の要求を読む）

- 検査の順序は固定: ① `buf.len() > MAX_REQUEST_BYTES` なら `TooLarge` ② ヘッダの終わり（**最初の** `\r\n\r\n`）がまだ無ければ `Incomplete`（本文に `\r\n\r\n` があっても最初のものが終わり。改行が `\n` だけの要求は終わりが見つからず、①で打ち切られる） ③ ヘッダ部（先頭から最初の `\r\n\r\n` まで）が UTF-8 でなければ `Malformed` ④ 要求行 ⑤ ヘッダ（下の規則。**全部のヘッダ行を最後まで検査し、1 つでも違反があれば `Malformed`**。`Content-Length` が `usize` に収まらないかどうかはここでは見ない） ⑥ `Content-Length` が `usize` に収まらなければ `TooLarge`。**ヘッダの長さ（先頭から最初の `\r\n\r\n` の直後まで＝終わりの 4 バイトを含む）＋`Content-Length`** が `MAX_REQUEST_BYTES` を超えるなら（ちょうど等しいときは超えない）、本文を待たずに `TooLarge` ⑦ 本文が足りなければ `Incomplete`
- 1 行目は `METHOD SP PATH SP HTTP/1.x`。**半角空白 1 つずつでちょうど 3 つに分ける**（空白が続く・3 つでないなら `Malformed`）。3 つ目が `HTTP/1.` で始まらない、PATH が `/` で始まらないなら `Malformed`。PATH はクエリ込みでそのまま入れる
- ヘッダ行は `名前:値`。`:` が無い行、名前が空の行（行頭が `:`）、名前の前後に空白がある行（`Content-Length : 5`）は `Malformed`。ヘッダ名は大文字小文字を区別しない。値は前後の ASCII の空白（SP と HT）だけを除く
- `Content-Length` の値は、前後の SP・HT を除いた後が ASCII の数字だけ（符号・空文字は `Malformed`）。`usize` に収まらない値は ⑥ で `TooLarge`。無ければ本文は空
- `Content-Length`・`Authorization`・`Origin` が 2 回以上あれば（値が同じでも）`Malformed`
- `Authorization`・`Origin` は値を入れる（値が空なら `Some("")`）。無ければ `None`
- `Transfer-Encoding` があれば（本文が届いていなくても）`Malformed`
- `Host`・`Connection` などその他のヘッダは読み捨てる
- 本文は `Content-Length` バイトだけ（余った分は捨てる）

### 8. `format_response` と `format_request`

- `format_response(status, body)`: `HTTP/1.1 <status> <reason>\r\nContent-Type: application/json\r\nContent-Length: <バイト数>\r\nConnection: close\r\n\r\n<body>`。reason は 200=`OK`・400=`Bad Request`・401=`Unauthorized`・403=`Forbidden`・404=`Not Found`・413=`Payload Too Large`・それ以外=`Error`
- `format_request(method, path, token, body)`: `<method> <path> HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer <token>\r\nContent-Type: application/json\r\nContent-Length: <バイト数>\r\nConnection: close\r\n\r\n<body>`
- `format_request` の出力は `parse_request` でそのまま読める（往復できる）。前提は `token`・`method`・`path` が空でなく、空白・CR・LF を含まず、`path` が `/` で始まること（それ以外は未定義でテストしない）

### 9. `is_authorized`

- `Origin` ヘッダが付いた要求は常に false（ブラウザのページから loopback を叩かれる経路を断つ）
- `authorization` が `Bearer <token>` と完全に一致するときだけ true（`Bearer` の大文字小文字も区別する）。`authorization` が `None` なら false。`token` が空文字のときは常に false。`Origin` が `Some("")`（空の値）でも「付いた」扱いで false
- 比較は長さを先に見て、同じ長さなら全バイトを比べ終えてから結果を返す（途中で抜けない）。**これは時間を測らないとテストできないので受け入れの条件に数えない**

## 範囲外

- ソケット・ファイル・プロセスの扱い（`pet-hook` とアプリの仕事）
- 承認の id の生成・待ち時間・吹き出しの表示
- `updatedInput`・`updatedPermissions`・`interrupt`（使わない）

## 受け入れ

- `cargo test -p pet-core` が通る（既存の 74 本も含む）
- `cargo clippy --workspace --all-targets -- -D warnings` が通る
- 上の 1〜9 の各節に少なくとも 1 本ずつテスト。境界（ちょうど `MAX_INPUT_BYTES`・ちょうど `max_chars`・`Content-Length` が足りない）と、壊しにいくケース（必須項目の欠落の順序・プロンプト本文が漏れない・`Origin` 付き・`Bearer` の大小・制御文字と改行・絵文字を含む切り詰め）を入れる
