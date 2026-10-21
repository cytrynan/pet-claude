//! Claude Code の hook 入力の解析・要約・出力の組み立て。
//!
//! `pet-hook.exe` は標準入力に渡された JSON をここで解析し、アプリへ送る。承認待ち
//! （`PermissionRequest`）だけは、アプリの答えを許可・拒否の JSON にして標準出力へ返す。
//! OS にもネットワークにも触らない純粋な関数だけを持つ。入力は全部信頼できない外部データ
//! なので、どの経路でも panic しない。
//! 仕様の正本は `.steering/20261021-phase2-hooks/requirements.md`。

use serde_json::{Map, Value, json};

use crate::behavior::AgentEvent;

/// 標準入力として受け付ける最大のバイト数。これを超えたら解析しない。
///
/// hook は会話の本文を含む JSON を渡してくる。上限が無いと巨大な入力で hook 側のメモリを
/// 食うので、ペットに要らない分を読む前に打ち切る。
pub const MAX_INPUT_BYTES: usize = 65_536;

/// 承認の吹き出しに出す要約の最大文字数（Unicode のスカラー値で数える）。
pub const SUMMARY_MAX_CHARS: usize = 120;

/// `Notification` の種類（`notification_type` の値）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationKind {
    /// `"permission_prompt"`
    PermissionPrompt,
    /// `"idle_prompt"`
    IdlePrompt,
    /// `"elicitation_dialog"`
    ElicitationDialog,
    /// それ以外の値（値はそのまま）。
    Other(String),
}

impl NotificationKind {
    /// `notification_type` の文字列に戻す。`Other` は中身をそのまま返す。
    pub fn as_str(&self) -> &str {
        match self {
            Self::PermissionPrompt => "permission_prompt",
            Self::IdlePrompt => "idle_prompt",
            Self::ElicitationDialog => "elicitation_dialog",
            Self::Other(s) => s,
        }
    }
}

/// 解析した hook 入力。プロンプトや応答の本文は持たない（持たせないことで漏れを構造的に防ぐ）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookInput {
    /// `UserPromptSubmit`
    Prompt { session: String },
    /// `PreToolUse`
    ToolStart { session: String, tool: String },
    /// `PermissionRequest`。`summary` は `summarize` を通した 1 行。
    PermissionRequest {
        session: String,
        tool: String,
        summary: String,
    },
    /// `Notification`
    Notification {
        session: String,
        kind: NotificationKind,
    },
    /// `Stop`
    Stop { session: String },
    /// `StopFailure`
    StopFailure { session: String, error: String },
    /// 上以外の `hook_event_name`。
    Other { session: String, event: String },
}

/// `parse` の失敗。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookError {
    /// 入力が `MAX_INPUT_BYTES` を超えた。
    TooLarge,
    /// JSON として読めない、またはオブジェクトでない。
    InvalidJson,
    /// 必須の項目が無い・文字列でない（項目名を入れる）。
    MissingField(&'static str),
}

/// 承認の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny {
        message: String,
    },
    /// 決めない（Claude Code のいつもの承認の流れに戻す）。
    Ask,
}

/// オブジェクトの項目を、文字列のときだけ取り出す。無い・null・文字列以外は `None`。
/// 空文字は「ある」として返す。
fn str_field<'a>(obj: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    obj.get(key).and_then(Value::as_str)
}

/// hook の標準入力を解析する。
///
/// 検査の順序は `TooLarge` → `InvalidJson` → `hook_event_name` → `session_id` →
/// イベント別の必須項目で固定し、最初に見つかった誤りだけを返す。
pub fn parse(input: &[u8]) -> Result<HookInput, HookError> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(HookError::TooLarge);
    }
    // from_slice は UTF-8 でない入力・BOM・末尾の余分な文字をすべてエラーにする。
    let Ok(Value::Object(obj)) = serde_json::from_slice::<Value>(input) else {
        return Err(HookError::InvalidJson);
    };

    let event =
        str_field(&obj, "hook_event_name").ok_or(HookError::MissingField("hook_event_name"))?;
    let session = str_field(&obj, "session_id")
        .ok_or(HookError::MissingField("session_id"))?
        .to_string();

    let tool_name = || {
        str_field(&obj, "tool_name")
            .map(str::to_string)
            .ok_or(HookError::MissingField("tool_name"))
    };

    let parsed = match event {
        "UserPromptSubmit" => HookInput::Prompt { session },
        "PreToolUse" => HookInput::ToolStart {
            session,
            tool: tool_name()?,
        },
        "PermissionRequest" => {
            let tool = tool_name()?;
            // tool_input が無い・オブジェクトでないときは Null を渡す（summarize は空文字を返す）。
            let null = Value::Null;
            let tool_input = match obj.get("tool_input") {
                Some(v @ Value::Object(_)) => v,
                _ => &null,
            };
            let summary = summarize(&tool, tool_input);
            HookInput::PermissionRequest {
                session,
                tool,
                summary,
            }
        }
        "Notification" => {
            let kind = match str_field(&obj, "notification_type").unwrap_or("") {
                "permission_prompt" => NotificationKind::PermissionPrompt,
                "idle_prompt" => NotificationKind::IdlePrompt,
                "elicitation_dialog" => NotificationKind::ElicitationDialog,
                other => NotificationKind::Other(other.to_string()),
            };
            HookInput::Notification { session, kind }
        }
        "Stop" => HookInput::Stop { session },
        "StopFailure" => HookInput::StopFailure {
            session,
            error: str_field(&obj, "error").unwrap_or("unknown").to_string(),
        },
        other => HookInput::Other {
            session,
            event: other.to_string(),
        },
    };
    Ok(parsed)
}

/// 承認の吹き出しに出す 1 行の要約。
///
/// ツールごとに決めた項目（コマンド・パス・URL など）だけを取り出す。`Write` の `content` や
/// `Edit` の差分のような本文は、表に無いので取り出さない（画面と通信に本文を流さないため）。
/// `mcp__` で始まる名前と未知のツールは何も取り出さず空文字を返す。
pub fn summarize(tool: &str, tool_input: &Value) -> String {
    summary_source(tool, tool_input)
        .map_or_else(String::new, |v| sanitize_line(v, SUMMARY_MAX_CHARS))
}

/// 要約の元にする `tool_input` の文字列（ツールごとに 1 項目）。無ければ None。
fn summary_source<'a>(tool: &str, tool_input: &'a Value) -> Option<&'a str> {
    let key = match tool {
        "Bash" | "PowerShell" => "command",
        "Read" | "Write" | "Edit" | "MultiEdit" => "file_path",
        "NotebookEdit" => "notebook_path",
        "WebFetch" => "url",
        "WebSearch" => "query",
        "Grep" | "Glob" => "pattern",
        "Agent" | "Task" => "description",
        _ => return None,
    };
    tool_input.get(key).and_then(Value::as_str)
}

/// 吹き出しの要約が、その操作の全部をそのまま見せているか。吹き出しから一押しで許可してよいのは
/// これが true のときだけ（2026-10-21 セキュリティレビューと検証係の指摘を 3 つ合わせた条件）:
///
/// 1. 要約が操作の全部になるツールで、`tool_input` に見えない項目が無い（`SHOWN_WHOLE`）。
///    Agent の prompt・Write の本文・Bash の `dangerouslyDisableSandbox` などは要約に出ないので false
/// 2. 要約の元が加工されていない（改行・タブ・空白の並び・前後の空白・切り詰めのどれも無い。
///    `echo safe⏎whoami` は要約では `echo safe whoami` に見えるが、実際は 2 つのコマンドになる）
/// 3. 見た目どおりに読める文字だけ（`is_plain_char`。見えない文字・空白に見える文字・結合文字は false）
pub fn summary_is_exact(tool: &str, tool_input: &Value) -> bool {
    let Some(allowed_keys) = SHOWN_WHOLE
        .iter()
        .find(|(name, _)| *name == tool)
        .map(|(_, keys)| *keys)
    else {
        return false;
    };
    let Some(obj) = tool_input.as_object() else {
        return false;
    };
    if obj.keys().any(|k| !allowed_keys.contains(&k.as_str())) {
        return false;
    }
    summary_source(tool, tool_input).is_some_and(|v| {
        !v.is_empty() && v.chars().all(is_plain_char) && sanitize_line(v, SUMMARY_MAX_CHARS) == v
    })
}

/// 要約が操作の全部になるツールと、`tool_input` に含まれてよい項目（要約の項目＋実行の中身を変えない項目）。
/// ここに無いツール・項目が 1 つでもあれば「ターミナルで」答えてもらう。
const SHOWN_WHOLE: [(&str, &[&str]); 4] = [
    // description は人向けの説明、timeout は待ち時間。どちらも実行するコマンドを変えない
    ("Bash", &["command", "description", "timeout"]),
    ("PowerShell", &["command", "description", "timeout"]),
    // offset・limit は読む範囲（読むだけなので許す）
    ("Read", &["file_path", "offset", "limit"]),
    ("WebSearch", &["query"]),
];

/// 見た目どおりに読める文字か（許可リスト）。拒否リストでは漏れが出たので（U+3164 の Hangul filler・
/// 異体字セレクタ・Tag 文字などが通った）、ASCII の印字可能文字・Latin-1 の記号と文字・日本語の
/// かなと漢字・全角英数と記号・CJK の句読点だけを通す。空白は半角空白だけ。アプリ側の `ui::can_allow` も
/// この関数を使う（集合を 1 か所に寄せる）。
pub fn is_plain_char(c: char) -> bool {
    matches!(c,
        ' '..='~'
        | '\u{00A1}'..='\u{00AC}' | '\u{00AE}'..='\u{00FF}'
        // CJK の句読点は使うものだけ（、。〃〄々〆〇・括弧類）。302A–302F の結合マーク・303F の
        // 半角幅の空欄などが混ざる範囲指定はしない（2026-10-21 検証係の指摘）
        | '\u{3001}'..='\u{3007}' | '\u{3008}'..='\u{3011}' | '\u{3014}'..='\u{301F}'
        | '\u{3041}'..='\u{3096}' | '\u{309B}'..='\u{30FF}'
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{FF01}'..='\u{FF5E}')
}

/// 表示用に 1 行へ整える。
///
/// 制御文字（改行・タブを含む）を空白にし、空白の連続を半角空白 1 つにまとめ、前後を除く。
/// 結果が `max_chars` 文字（Unicode のスカラー値）を超えたら、先頭 `max_chars - 1` 文字と
/// `…` にする。切り詰めた後はもう一度 trim しない。
pub fn sanitize_line(s: &str, max_chars: usize) -> String {
    let replaced: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    // split_whitespace は char::is_whitespace で分けるので、空白の並び（長さ 1 の非 ASCII 空白を
    // 含む）が半角空白 1 つになり、前後の空白も落ちる。
    let collapsed = replaced.split_whitespace().collect::<Vec<_>>().join(" ");

    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    if max_chars == 0 {
        return String::new();
    }
    let mut out: String = collapsed.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

/// ペットの行動ロジックへ渡す出来事に直す。ペットが反応しない入力は `None`。
pub fn to_agent_event(input: &HookInput) -> Option<AgentEvent> {
    match input {
        HookInput::Prompt { session } => Some(AgentEvent::Prompt {
            session: session.clone(),
        }),
        HookInput::ToolStart { session, .. } => Some(AgentEvent::ToolStart {
            session: session.clone(),
        }),
        HookInput::PermissionRequest { session, .. } => Some(AgentEvent::NeedsInput {
            session: session.clone(),
        }),
        HookInput::Notification { session, kind } => match kind {
            NotificationKind::PermissionPrompt | NotificationKind::ElicitationDialog => {
                Some(AgentEvent::NeedsInput {
                    session: session.clone(),
                })
            }
            NotificationKind::IdlePrompt | NotificationKind::Other(_) => None,
        },
        HookInput::Stop { session } => Some(AgentEvent::Done {
            session: session.clone(),
        }),
        HookInput::StopFailure { session, .. } => Some(AgentEvent::Error {
            session: session.clone(),
        }),
        HookInput::Other { .. } => None,
    }
}

/// `PermissionRequest` の hook が標準出力に書く JSON（改行を含まない 1 行）。
///
/// `Ask` は `{}`（何も決めず、Claude Code のいつもの承認の流れに戻す）。
pub fn permission_output(decision: &Decision) -> String {
    let value = match decision {
        Decision::Allow => json!({
            "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": { "behavior": "allow" }
            }
        }),
        Decision::Deny { message } => json!({
            "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": { "behavior": "deny", "message": message }
            }
        }),
        Decision::Ask => json!({}),
    };
    // Value の文字列化は失敗しない。万一でも「決めない」に倒す。
    serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_is_exact_only_when_nothing_changed() {
        let bash = |c: &str| json!({ "command": c });
        assert!(summary_is_exact("Bash", &bash("git status")));
        // 改行・タブ・空白の並び・前後の空白・切り詰め・空・要約の無いツールは false
        assert!(!summary_is_exact("Bash", &bash("echo safe\nwhoami")));
        assert!(!summary_is_exact("Bash", &bash("a\tb")));
        assert!(!summary_is_exact("Bash", &bash("a  b")));
        assert!(!summary_is_exact("Bash", &bash(" a")));
        assert!(!summary_is_exact(
            "Bash",
            &bash(&"x".repeat(SUMMARY_MAX_CHARS + 1))
        ));
        assert!(summary_is_exact(
            "Bash",
            &bash(&"x".repeat(SUMMARY_MAX_CHARS))
        ));
        assert!(!summary_is_exact("Bash", &bash("")));
        assert!(!summary_is_exact("mcp__x__y", &json!({ "command": "ls" })));
        assert!(!summary_is_exact("Bash", &Value::Null));
        // 見えない書式文字（右から左・ゼロ幅・BOM）を含むものも false
        assert!(!summary_is_exact("Bash", &bash("echo \u{202E}txt.exe")));
        assert!(!summary_is_exact("Bash", &bash("rm\u{200B} -rf")));
        assert!(!summary_is_exact("Bash", &bash("\u{FEFF}ls")));
    }

    #[test]
    fn summary_must_be_the_whole_operation() {
        // 要約に出ない項目があるツール・項目は false（本文・副エージェントへの指示・サンドボックス解除）
        assert!(!summary_is_exact(
            "Edit",
            &json!({ "file_path": "C:/a.txt" })
        ));
        assert!(!summary_is_exact(
            "Write",
            &json!({ "file_path": "package.json", "content": "x" })
        ));
        assert!(!summary_is_exact(
            "Agent",
            &json!({ "description": "Run tests", "prompt": "do anything" })
        ));
        assert!(!summary_is_exact(
            "Bash",
            &json!({ "command": "ls", "dangerouslyDisableSandbox": true })
        ));
        assert!(!summary_is_exact(
            "Bash",
            &json!({ "command": "ls", "run_in_background": true })
        ));
        // 実行の中身を変えない項目だけなら true
        assert!(summary_is_exact(
            "Bash",
            &json!({ "command": "git status", "description": "状態を見る", "timeout": 1000 })
        ));
        assert!(summary_is_exact(
            "Read",
            &json!({ "file_path": "C:/a.txt", "limit": 10 })
        ));
        assert!(summary_is_exact(
            "WebSearch",
            &json!({ "query": "rust 2024" })
        ));
    }

    #[test]
    fn only_plainly_visible_chars_are_exact() {
        let bash = |c: &str| json!({ "command": c });
        assert!(summary_is_exact(
            "Bash",
            &bash("echo こんにちは、世界！ ＡＢＣ")
        ));
        assert!(summary_is_exact("Bash", &bash("echo 「人々」〇〆。")));
        // 空白に見える・見えない・結合する文字（拒否リストでは漏れていたもの）
        for c in [
            '\u{3164}',
            '\u{FFA0}',
            '\u{2800}',
            '\u{E0041}',
            '\u{FE0F}',
            '\u{034F}',
            '\u{0338}',
            '\u{3000}',
            '\u{00A0}',
            '\u{00AD}',
            '\u{1F600}',
            // CJK の句読点の範囲に混ざっていた結合マーク・空欄・変異指示子
            '\u{302A}',
            '\u{302F}',
            '\u{303E}',
            '\u{303F}',
        ] {
            assert!(
                !summary_is_exact("Bash", &bash(&format!("git checkout main{c}"))),
                "U+{:04X}",
                c as u32
            );
        }
    }
}
