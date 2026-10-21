//! `pet_core::hook` の受け入れテスト（テスト先行）。
//!
//! 原本: `.steering/20261021-phase2-hooks/requirements.md` の 1〜5 節。公開 API だけを使う。

use pet_core::behavior::AgentEvent;
use pet_core::hook::{
    Decision, HookError, HookInput, MAX_INPUT_BYTES, NotificationKind, SUMMARY_MAX_CHARS, parse,
    permission_output, sanitize_line, summarize, to_agent_event,
};
use pet_core::ipc::WireEvent;
use serde_json::{Map, Value, json};

fn parse_value(v: &Value) -> Result<HookInput, HookError> {
    parse(v.to_string().as_bytes())
}

fn st(x: &str) -> String {
    x.to_string()
}

/// `v` を JSON にし、末尾を半角空白で埋めてちょうど `total` バイトにする。
fn padded(v: &Value, total: usize) -> Vec<u8> {
    let mut b = v.to_string().into_bytes();
    assert!(b.len() <= total, "土台の JSON が長すぎる");
    b.resize(total, b' ');
    b
}

// ---------------------------------------------------------------------------
// 1. parse
// ---------------------------------------------------------------------------

// 要件 識別子契約: 定数の値
#[test]
fn constants_have_contract_values() {
    assert_eq!(MAX_INPUT_BYTES, 65_536);
    assert_eq!(SUMMARY_MAX_CHARS, 120);
}

// 要件 1: UserPromptSubmit → Prompt
#[test]
fn parse_user_prompt_submit() {
    let r = parse_value(&json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "s1",
        "prompt": "hello"
    }));
    assert_eq!(r, Ok(HookInput::Prompt { session: st("s1") }));
}

// 要件 1: PreToolUse → ToolStart { tool: tool_name }
#[test]
fn parse_pre_tool_use() {
    let r = parse_value(&json!({
        "hook_event_name": "PreToolUse",
        "session_id": "s1",
        "tool_name": "Bash",
        "tool_input": { "command": "ls" }
    }));
    assert_eq!(
        r,
        Ok(HookInput::ToolStart {
            session: st("s1"),
            tool: st("Bash"),
        })
    );
}

// 要件 1: PermissionRequest → summary は summarize(tool_name, tool_input)
#[test]
fn parse_permission_request_uses_summarize() {
    let tool_input = json!({ "command": "git   status\n--short", "description": "show status" });
    let r = parse_value(&json!({
        "hook_event_name": "PermissionRequest",
        "session_id": "s1",
        "tool_name": "Bash",
        "tool_input": tool_input
    }));
    assert_eq!(
        r,
        Ok(HookInput::PermissionRequest {
            session: st("s1"),
            tool: st("Bash"),
            summary: summarize("Bash", &tool_input),
        })
    );
    assert_eq!(
        r,
        Ok(HookInput::PermissionRequest {
            session: st("s1"),
            tool: st("Bash"),
            summary: st("git status --short"),
        })
    );
}

// 要件 1: PermissionRequest の tool_input が無い・オブジェクトでない → summarize に Null
#[test]
fn parse_permission_request_without_object_tool_input() {
    let cases = [
        None,
        Some(Value::Null),
        Some(json!("ls -la")),
        Some(json!(["command", "ls"])),
        Some(json!(42)),
        Some(json!(true)),
    ];
    for tool_input in cases {
        let mut obj = Map::new();
        obj.insert(st("hook_event_name"), json!("PermissionRequest"));
        obj.insert(st("session_id"), json!("s"));
        obj.insert(st("tool_name"), json!("Bash"));
        if let Some(ti) = &tool_input {
            obj.insert(st("tool_input"), ti.clone());
        }
        let r = parse_value(&Value::Object(obj));
        assert_eq!(
            r,
            Ok(HookInput::PermissionRequest {
                session: st("s"),
                tool: st("Bash"),
                summary: summarize("Bash", &Value::Null),
            }),
            "tool_input = {tool_input:?}"
        );
        assert_eq!(
            r,
            Ok(HookInput::PermissionRequest {
                session: st("s"),
                tool: st("Bash"),
                summary: String::new(),
            }),
            "tool_input = {tool_input:?}"
        );
    }
}

// 要件 1: Notification の notification_type → NotificationKind
#[test]
fn parse_notification_kinds() {
    let cases = [
        ("permission_prompt", NotificationKind::PermissionPrompt),
        ("idle_prompt", NotificationKind::IdlePrompt),
        ("elicitation_dialog", NotificationKind::ElicitationDialog),
        ("auth_success", NotificationKind::Other(st("auth_success"))),
        // 値はそのまま照合する（大文字小文字や前後の空白が違えば Other）
        (
            "Permission_Prompt",
            NotificationKind::Other(st("Permission_Prompt")),
        ),
        (" idle_prompt", NotificationKind::Other(st(" idle_prompt"))),
        ("", NotificationKind::Other(String::new())),
    ];
    for (value, kind) in cases {
        let r = parse_value(&json!({
            "hook_event_name": "Notification",
            "session_id": "s",
            "notification_type": value,
            "message": "m",
            "title": "t"
        }));
        assert_eq!(
            r,
            Ok(HookInput::Notification {
                session: st("s"),
                kind,
            }),
            "notification_type = {value:?}"
        );
    }
}

// 要件 1: Notification の notification_type が「無い」（欠落・null・文字列でない）→ Other("")
#[test]
fn parse_notification_without_type_is_other_empty() {
    let cases = [
        None,
        Some(Value::Null),
        Some(json!(1)),
        Some(json!(false)),
        Some(json!(["permission_prompt"])),
        Some(json!({ "type": "permission_prompt" })),
    ];
    for nt in cases {
        let mut obj = Map::new();
        obj.insert(st("hook_event_name"), json!("Notification"));
        obj.insert(st("session_id"), json!("s"));
        if let Some(v) = &nt {
            obj.insert(st("notification_type"), v.clone());
        }
        assert_eq!(
            parse_value(&Value::Object(obj)),
            Ok(HookInput::Notification {
                session: st("s"),
                kind: NotificationKind::Other(String::new()),
            }),
            "notification_type = {nt:?}"
        );
    }
}

// 要件 1: Stop → Stop
#[test]
fn parse_stop() {
    let r = parse_value(&json!({
        "hook_event_name": "Stop",
        "session_id": "s9",
        "stop_hook_active": false,
        "last_assistant_message": "done"
    }));
    assert_eq!(r, Ok(HookInput::Stop { session: st("s9") }));
}

// 要件 1: StopFailure の error（無ければ "unknown"、空文字はそのまま）
#[test]
fn parse_stop_failure_error() {
    let cases = [
        (Some(json!("rate_limit")), "rate_limit"),
        (Some(json!("")), ""),
        (None, "unknown"),
        (Some(Value::Null), "unknown"),
        (Some(json!(500)), "unknown"),
        (Some(json!(true)), "unknown"),
        (Some(json!(["rate_limit"])), "unknown"),
        (Some(json!({ "code": "rate_limit" })), "unknown"),
    ];
    for (err, expected) in cases {
        let mut obj = Map::new();
        obj.insert(st("hook_event_name"), json!("StopFailure"));
        obj.insert(st("session_id"), json!("s"));
        if let Some(v) = &err {
            obj.insert(st("error"), v.clone());
        }
        assert_eq!(
            parse_value(&Value::Object(obj)),
            Ok(HookInput::StopFailure {
                session: st("s"),
                error: st(expected),
            }),
            "error = {err:?}"
        );
    }
}

// 要件 1: 表に無い hook_event_name → Other { event }（空文字の名前も「ある」）
#[test]
fn parse_other_events() {
    for event in ["SessionStart", "PostToolUse", "SubagentStop", "stop", ""] {
        let r = parse_value(&json!({
            "hook_event_name": event,
            "session_id": "s",
            "tool_name": "Bash"
        }));
        assert_eq!(
            r,
            Ok(HookInput::Other {
                session: st("s"),
                event: st(event),
            }),
            "event = {event:?}"
        );
    }
}

// 要件 1: 空文字の session_id・tool_name は「ある」としてそのまま使う
#[test]
fn parse_empty_strings_are_present() {
    assert_eq!(
        parse_value(&json!({ "hook_event_name": "Stop", "session_id": "" })),
        Ok(HookInput::Stop {
            session: String::new()
        })
    );
    assert_eq!(
        parse_value(&json!({
            "hook_event_name": "PreToolUse",
            "session_id": "",
            "tool_name": ""
        })),
        Ok(HookInput::ToolStart {
            session: String::new(),
            tool: String::new(),
        })
    );
    assert_eq!(
        parse_value(&json!({
            "hook_event_name": "PermissionRequest",
            "session_id": "s",
            "tool_name": ""
        })),
        Ok(HookInput::PermissionRequest {
            session: st("s"),
            tool: String::new(),
            summary: String::new(),
        })
    );
}

// 要件 1: ちょうど MAX_INPUT_BYTES は解析し、1 バイト超えたら TooLarge
#[test]
fn parse_size_boundary() {
    let base = json!({ "hook_event_name": "Stop", "session_id": "s" });
    let exact = padded(&base, MAX_INPUT_BYTES);
    assert_eq!(exact.len(), MAX_INPUT_BYTES);
    assert_eq!(parse(&exact), Ok(HookInput::Stop { session: st("s") }));

    let over = padded(&base, MAX_INPUT_BYTES + 1);
    assert_eq!(parse(&over), Err(HookError::TooLarge));

    // 中身の大きい本物の入力でも境界は同じ（空白でなく文字列で埋める）
    let filler_len = MAX_INPUT_BYTES
        - json!({ "hook_event_name": "Stop", "session_id": "s", "prompt": "" })
            .to_string()
            .len();
    let exact_real = json!({
        "hook_event_name": "Stop",
        "session_id": "s",
        "prompt": "p".repeat(filler_len)
    })
    .to_string();
    assert_eq!(exact_real.len(), MAX_INPUT_BYTES);
    assert_eq!(
        parse(exact_real.as_bytes()),
        Ok(HookInput::Stop { session: st("s") })
    );
    let over_real = json!({
        "hook_event_name": "Stop",
        "session_id": "s",
        "prompt": "p".repeat(filler_len + 1)
    })
    .to_string();
    assert_eq!(over_real.len(), MAX_INPUT_BYTES + 1);
    assert_eq!(parse(over_real.as_bytes()), Err(HookError::TooLarge));
}

// 要件 1: 検査の順序 TooLarge が先（壊れた JSON でも大きければ TooLarge）
#[test]
fn parse_too_large_checked_before_json() {
    let garbage = vec![b'x'; MAX_INPUT_BYTES + 1];
    assert_eq!(parse(&garbage), Err(HookError::TooLarge));
    let mut invalid_utf8 = vec![0xFF_u8; MAX_INPUT_BYTES + 1];
    invalid_utf8[0] = b'{';
    assert_eq!(parse(&invalid_utf8), Err(HookError::TooLarge));
    // 必須項目が欠けた JSON でも大きければ TooLarge
    let missing = padded(&json!({}), MAX_INPUT_BYTES + 1);
    assert_eq!(parse(&missing), Err(HookError::TooLarge));
}

// 要件 1: オブジェクトでない JSON・読めない入力は InvalidJson
#[test]
fn parse_invalid_json() {
    let cases: [&[u8]; 14] = [
        b"",
        b"   \r\n\t ",
        b"not json",
        b"{",
        b"{\"hook_event_name\":\"Stop\",\"session_id\":\"s\"",
        b"[]",
        b"[{\"hook_event_name\":\"Stop\",\"session_id\":\"s\"}]",
        b"\"Stop\"",
        b"42",
        b"true",
        b"null",
        b"{}{}",
        b"{\"hook_event_name\":\"Stop\",\"session_id\":\"s\"} x",
        b"{'hook_event_name':'Stop','session_id':'s'}",
    ];
    for c in cases {
        assert_eq!(
            parse(c),
            Err(HookError::InvalidJson),
            "input = {:?}",
            String::from_utf8_lossy(c)
        );
    }
}

// 要件 1: UTF-8 でない入力は InvalidJson（使わない項目の中でも）
#[test]
fn parse_invalid_utf8_is_invalid_json() {
    let in_session = b"{\"hook_event_name\":\"Stop\",\"session_id\":\"\xFF\"}";
    assert_eq!(parse(in_session), Err(HookError::InvalidJson));
    let in_unused = b"{\"hook_event_name\":\"Stop\",\"session_id\":\"s\",\"prompt\":\"\xC3\x28\"}";
    assert_eq!(parse(in_unused), Err(HookError::InvalidJson));
}

// 要件 1: 先頭の UTF-8 BOM は InvalidJson
#[test]
fn parse_bom_is_invalid_json() {
    let mut b = vec![0xEF, 0xBB, 0xBF];
    b.extend_from_slice(
        json!({ "hook_event_name": "Stop", "session_id": "s" })
            .to_string()
            .as_bytes(),
    );
    assert_eq!(parse(&b), Err(HookError::InvalidJson));
}

// 要件 1: JSON の前後の空白・改行は許す
#[test]
fn parse_allows_surrounding_whitespace() {
    let body = json!({ "hook_event_name": "Stop", "session_id": "s" }).to_string();
    for (pre, post) in [
        (" ", ""),
        ("", "\n"),
        ("\r\n\t  ", "\r\n\r\n"),
        ("\n\n", "  \t"),
    ] {
        let input = format!("{pre}{body}{post}");
        assert_eq!(
            parse(input.as_bytes()),
            Ok(HookInput::Stop { session: st("s") }),
            "input = {input:?}"
        );
    }
}

// 要件 1: 検査の順序 hook_event_name → session_id → イベント別の必須項目
#[test]
fn parse_missing_field_order() {
    // どちらも無い → hook_event_name
    assert_eq!(
        parse_value(&json!({})),
        Err(HookError::MissingField("hook_event_name"))
    );
    assert_eq!(
        parse_value(&json!({ "tool_name": "Bash" })),
        Err(HookError::MissingField("hook_event_name"))
    );
    // session_id だけある → hook_event_name
    assert_eq!(
        parse_value(&json!({ "session_id": "s" })),
        Err(HookError::MissingField("hook_event_name"))
    );
    // hook_event_name だけある → session_id（イベント別の必須項目より先）
    assert_eq!(
        parse_value(&json!({ "hook_event_name": "PreToolUse" })),
        Err(HookError::MissingField("session_id"))
    );
    assert_eq!(
        parse_value(&json!({ "hook_event_name": "PermissionRequest" })),
        Err(HookError::MissingField("session_id"))
    );
    // 表に無いイベントでも session_id は必須
    assert_eq!(
        parse_value(&json!({ "hook_event_name": "SessionStart" })),
        Err(HookError::MissingField("session_id"))
    );
    assert_eq!(
        parse_value(&json!({ "hook_event_name": "Stop" })),
        Err(HookError::MissingField("session_id"))
    );
    // 両方ある・tool_name が無い → tool_name
    assert_eq!(
        parse_value(&json!({ "hook_event_name": "PreToolUse", "session_id": "s" })),
        Err(HookError::MissingField("tool_name"))
    );
    assert_eq!(
        parse_value(&json!({
            "hook_event_name": "PermissionRequest",
            "session_id": "s",
            "tool_input": { "command": "ls" }
        })),
        Err(HookError::MissingField("tool_name"))
    );
}

// 要件 1: 「無い」は欠落・null・文字列でない（数値・真偽・配列・オブジェクト）
#[test]
fn parse_non_string_fields_are_missing() {
    let non_strings = [
        Value::Null,
        json!(1),
        json!(1.5),
        json!(true),
        json!(["Stop"]),
        json!({ "name": "Stop" }),
    ];
    for bad in &non_strings {
        assert_eq!(
            parse_value(&json!({ "hook_event_name": bad, "session_id": "s" })),
            Err(HookError::MissingField("hook_event_name")),
            "hook_event_name = {bad:?}"
        );
        assert_eq!(
            parse_value(&json!({ "hook_event_name": "Stop", "session_id": bad })),
            Err(HookError::MissingField("session_id")),
            "session_id = {bad:?}"
        );
        // hook_event_name が壊れていれば session_id も壊れていても hook_event_name
        assert_eq!(
            parse_value(&json!({ "hook_event_name": bad, "session_id": bad })),
            Err(HookError::MissingField("hook_event_name")),
            "both = {bad:?}"
        );
        for event in ["PreToolUse", "PermissionRequest"] {
            assert_eq!(
                parse_value(&json!({
                    "hook_event_name": event,
                    "session_id": "s",
                    "tool_name": bad
                })),
                Err(HookError::MissingField("tool_name")),
                "{event} tool_name = {bad:?}"
            );
            // session_id も壊れていれば session_id が先
            assert_eq!(
                parse_value(&json!({
                    "hook_event_name": event,
                    "session_id": bad,
                    "tool_name": bad
                })),
                Err(HookError::MissingField("session_id")),
                "{event} session_id/tool_name = {bad:?}"
            );
        }
    }
}

// 要件 1: 使わないイベントでは tool_name が無くても誤りにしない
#[test]
fn parse_tool_name_only_required_for_tool_events() {
    for event in [
        "UserPromptSubmit",
        "Notification",
        "Stop",
        "StopFailure",
        "PostToolUse",
    ] {
        assert!(
            parse_value(&json!({ "hook_event_name": event, "session_id": "s" })).is_ok(),
            "event = {event}"
        );
    }
}

/// 入力を解析し、`Debug` と `WireEvent` の JSON のどちらにも `marks` が含まれないことを確かめる。
fn assert_no_leak(input: &Value, marks: &[&str]) -> HookInput {
    let parsed = parse_value(input).expect("解析できるはず");
    let debug = format!("{parsed:?}");
    let wire = serde_json::to_string(&WireEvent::from(&parsed)).expect("JSON にできるはず");
    for m in marks {
        assert!(!debug.contains(m), "Debug に {m} が漏れた: {debug}");
        assert!(!wire.contains(m), "WireEvent に {m} が漏れた: {wire}");
    }
    parsed
}

// 要件 1: UserPromptSubmit・PreToolUse でプロンプト本文や使わない項目を入れない（目印の検査）
#[test]
fn parse_does_not_leak_unused_fields_prompt_and_tool_start() {
    // UserPromptSubmit
    assert_no_leak(
        &json!({
            "hook_event_name": "UserPromptSubmit",
            "session_id": "s",
            "prompt": "MARK_PROMPT_A1",
            "transcript_path": "C:/MARK_TRANSCRIPT_A2.jsonl",
            "cwd": "C:/MARK_CWD_A3",
            "permission_mode": "MARK_MODE_A4"
        }),
        &[
            "MARK_PROMPT_A1",
            "MARK_TRANSCRIPT_A2",
            "MARK_CWD_A3",
            "MARK_MODE_A4",
        ],
    );
    // PreToolUse（Write の content・Edit の old/new）
    assert_no_leak(
        &json!({
            "hook_event_name": "PreToolUse",
            "session_id": "s",
            "tool_name": "Write",
            "tool_input": { "file_path": "C:/a.txt", "content": "MARK_CONTENT_B1" },
            "transcript_path": "MARK_TRANSCRIPT_B2"
        }),
        &["MARK_CONTENT_B1", "MARK_TRANSCRIPT_B2", "C:/a.txt"],
    );
    assert_no_leak(
        &json!({
            "hook_event_name": "PreToolUse",
            "session_id": "s",
            "tool_name": "Edit",
            "tool_input": {
                "file_path": "C:/b.txt",
                "old_string": "MARK_OLD_C1",
                "new_string": "MARK_NEW_C2"
            }
        }),
        &["MARK_OLD_C1", "MARK_NEW_C2"],
    );
}

// 要件 1: PermissionRequest で summarize が取り出さない項目を入れない（目印の検査）
#[test]
fn parse_does_not_leak_unused_fields_permission_request() {
    // PermissionRequest: Bash は command だけを取り出す（description は取り出さない）
    let p = assert_no_leak(
        &json!({
            "hook_event_name": "PermissionRequest",
            "session_id": "s",
            "tool_name": "Bash",
            "tool_input": { "command": "echo SAFE_CMD", "description": "MARK_DESC_D1" },
            "permission_suggestions": [{ "type": "addRules", "rules": ["MARK_SUGGEST_D2"] }],
            "transcript_path": "MARK_TRANSCRIPT_D3",
            "prompt": "MARK_PROMPT_D4"
        }),
        &[
            "MARK_DESC_D1",
            "MARK_SUGGEST_D2",
            "MARK_TRANSCRIPT_D3",
            "MARK_PROMPT_D4",
        ],
    );
    assert_eq!(
        p,
        HookInput::PermissionRequest {
            session: st("s"),
            tool: st("Bash"),
            summary: st("echo SAFE_CMD"),
        }
    );
    // PermissionRequest: Write は file_path だけ（content は出さない）
    assert_no_leak(
        &json!({
            "hook_event_name": "PermissionRequest",
            "session_id": "s",
            "tool_name": "Write",
            "tool_input": { "file_path": "C:/w.txt", "content": "MARK_CONTENT_E1" }
        }),
        &["MARK_CONTENT_E1"],
    );
    // PermissionRequest: Edit・MultiEdit の old/new は出さない
    assert_no_leak(
        &json!({
            "hook_event_name": "PermissionRequest",
            "session_id": "s",
            "tool_name": "Edit",
            "tool_input": {
                "file_path": "C:/e.txt",
                "old_string": "MARK_OLD_F1",
                "new_string": "MARK_NEW_F2"
            }
        }),
        &["MARK_OLD_F1", "MARK_NEW_F2"],
    );
    assert_no_leak(
        &json!({
            "hook_event_name": "PermissionRequest",
            "session_id": "s",
            "tool_name": "MultiEdit",
            "tool_input": {
                "file_path": "C:/m.txt",
                "edits": [{ "old_string": "MARK_OLD_G1", "new_string": "MARK_NEW_G2" }]
            }
        }),
        &["MARK_OLD_G1", "MARK_NEW_G2"],
    );
}

// 要件 1: PermissionRequest の mcp__・Agent で summarize が取り出さない項目を入れない（目印の検査）
#[test]
fn parse_does_not_leak_unused_fields_permission_request_mcp_and_agent() {
    // PermissionRequest: mcp__ は何も取り出さない
    let p = assert_no_leak(
        &json!({
            "hook_event_name": "PermissionRequest",
            "session_id": "s",
            "tool_name": "mcp__srv__run",
            "tool_input": {
                "command": "MARK_MCP_CMD_H1",
                "query": "MARK_MCP_Q_H2",
                "file_path": "MARK_MCP_PATH_H3",
                "description": "MARK_MCP_DESC_H4"
            }
        }),
        &[
            "MARK_MCP_CMD_H1",
            "MARK_MCP_Q_H2",
            "MARK_MCP_PATH_H3",
            "MARK_MCP_DESC_H4",
        ],
    );
    assert_eq!(
        p,
        HookInput::PermissionRequest {
            session: st("s"),
            tool: st("mcp__srv__run"),
            summary: String::new(),
        }
    );
    // PermissionRequest: Agent は description を取り出すが prompt は出さない
    let p = assert_no_leak(
        &json!({
            "hook_event_name": "PermissionRequest",
            "session_id": "s",
            "tool_name": "Agent",
            "tool_input": { "description": "review code", "prompt": "MARK_AGENT_PROMPT_I1" }
        }),
        &["MARK_AGENT_PROMPT_I1"],
    );
    assert_eq!(
        p,
        HookInput::PermissionRequest {
            session: st("s"),
            tool: st("Agent"),
            summary: st("review code"),
        }
    );
}

// 要件 1: Notification・Stop・StopFailure・Other で本文や使わない項目を入れない（目印の検査）
#[test]
fn parse_does_not_leak_unused_fields_other_events() {
    // Notification: message・title は出さない
    for nt in [
        "permission_prompt",
        "idle_prompt",
        "elicitation_dialog",
        "auth_success",
    ] {
        assert_no_leak(
            &json!({
                "hook_event_name": "Notification",
                "session_id": "s",
                "notification_type": nt,
                "message": "MARK_MSG_J1",
                "title": "MARK_TITLE_J2",
                "transcript_path": "MARK_TRANSCRIPT_J3"
            }),
            &["MARK_MSG_J1", "MARK_TITLE_J2", "MARK_TRANSCRIPT_J3"],
        );
    }
    // Stop: last_assistant_message は出さない
    assert_no_leak(
        &json!({
            "hook_event_name": "Stop",
            "session_id": "s",
            "stop_hook_active": true,
            "last_assistant_message": "MARK_LAST_K1",
            "transcript_path": "MARK_TRANSCRIPT_K2"
        }),
        &["MARK_LAST_K1", "MARK_TRANSCRIPT_K2"],
    );
    // StopFailure: error だけ。error_details・last_assistant_message は出さない
    let p = assert_no_leak(
        &json!({
            "hook_event_name": "StopFailure",
            "session_id": "s",
            "error": "rate_limit",
            "error_details": "MARK_ERRDETAIL_L1",
            "last_assistant_message": "MARK_LAST_L2"
        }),
        &["MARK_ERRDETAIL_L1", "MARK_LAST_L2"],
    );
    assert_eq!(
        p,
        HookInput::StopFailure {
            session: st("s"),
            error: st("rate_limit"),
        }
    );
    // Other: 名前以外は出さない（tool_input・tool_response・prompt）
    assert_no_leak(
        &json!({
            "hook_event_name": "PostToolUse",
            "session_id": "s",
            "tool_name": "Bash",
            "tool_input": { "command": "MARK_POST_CMD_M1" },
            "tool_response": { "stdout": "MARK_POST_OUT_M2" },
            "prompt": "MARK_PROMPT_M3"
        }),
        &["MARK_POST_CMD_M1", "MARK_POST_OUT_M2", "MARK_PROMPT_M3"],
    );
}

// 要件 1: summary 以外の文字列は加工せず、長さの上限も付けずにそのまま渡す
#[test]
fn parse_passes_strings_through_unmodified() {
    let session = format!("  s\n\t\u{0}\u{7}x{}  ", "あ".repeat(5_000));
    let tool = "Bash\r\n  tool\t";
    let error = format!("e\n\u{1b}[31m{}", "z".repeat(1_000));
    let event = "Weird\n Event\t";
    let other_kind = "kind\nwith  spaces ";

    assert_eq!(
        parse_value(&json!({
            "hook_event_name": "PreToolUse",
            "session_id": session,
            "tool_name": tool
        })),
        Ok(HookInput::ToolStart {
            session: session.clone(),
            tool: st(tool),
        })
    );
    assert_eq!(
        parse_value(&json!({
            "hook_event_name": "PermissionRequest",
            "session_id": session,
            "tool_name": tool
        })),
        Ok(HookInput::PermissionRequest {
            session: session.clone(),
            tool: st(tool),
            summary: String::new(),
        })
    );
    assert_eq!(
        parse_value(&json!({
            "hook_event_name": "StopFailure",
            "session_id": session,
            "error": error
        })),
        Ok(HookInput::StopFailure {
            session: session.clone(),
            error,
        })
    );
    assert_eq!(
        parse_value(&json!({ "hook_event_name": event, "session_id": session })),
        Ok(HookInput::Other {
            session: session.clone(),
            event: st(event),
        })
    );
    assert_eq!(
        parse_value(&json!({
            "hook_event_name": "Notification",
            "session_id": session,
            "notification_type": other_kind
        })),
        Ok(HookInput::Notification {
            session,
            kind: NotificationKind::Other(st(other_kind)),
        })
    );
    // 長い session（上限は MAX_INPUT_BYTES だけ）
    let long_session = "q".repeat(60_000);
    assert_eq!(
        parse_value(&json!({ "hook_event_name": "Stop", "session_id": long_session })),
        Ok(HookInput::Stop {
            session: long_session
        })
    );
}

// ---------------------------------------------------------------------------
// 2. summarize
// ---------------------------------------------------------------------------

/// 候補になりうる項目を全部、別々の値で埋めた `tool_input`。
fn all_fields_input() -> Value {
    json!({
        "command": "V_command",
        "file_path": "V_file_path",
        "notebook_path": "V_notebook_path",
        "url": "V_url",
        "query": "V_query",
        "pattern": "V_pattern",
        "description": "V_description",
        "prompt": "V_prompt",
        "content": "V_content",
        "path": "V_path"
    })
}

// 要件 2: ツールごとに表の項目だけを取り出す
#[test]
fn summarize_picks_field_per_tool() {
    let cases = [
        ("Bash", "V_command"),
        ("PowerShell", "V_command"),
        ("Read", "V_file_path"),
        ("Write", "V_file_path"),
        ("Edit", "V_file_path"),
        ("MultiEdit", "V_file_path"),
        ("NotebookEdit", "V_notebook_path"),
        ("WebFetch", "V_url"),
        ("WebSearch", "V_query"),
        ("Grep", "V_pattern"),
        ("Glob", "V_pattern"),
        ("Agent", "V_description"),
        ("Task", "V_description"),
    ];
    let input = all_fields_input();
    for (tool, expected) in cases {
        assert_eq!(summarize(tool, &input), expected, "tool = {tool}");
    }
}

// 要件 2: mcp__ で始まる名前・表に無い名前は空文字
#[test]
fn summarize_unknown_and_mcp_tools_are_empty() {
    let input = all_fields_input();
    for tool in [
        "mcp__srv__tool",
        "mcp__",
        "mcp__Bash",
        "mcp__github__search",
        "Foo",
        "bash",
        "BASH",
        "Bash ",
        " Bash",
        "TodoWrite",
        "LS",
        "",
    ] {
        assert_eq!(summarize(tool, &input), "", "tool = {tool:?}");
    }
}

// 要件 2: 値が無い・文字列でないときは空文字
#[test]
fn summarize_missing_or_non_string_is_empty() {
    for bad in [
        Value::Null,
        json!(1),
        json!(true),
        json!(["ls"]),
        json!({ "cmd": "ls" }),
    ] {
        assert_eq!(
            summarize("Bash", &json!({ "command": bad })),
            "",
            "command = {bad:?}"
        );
        assert_eq!(
            summarize("Read", &json!({ "file_path": bad })),
            "",
            "file_path = {bad:?}"
        );
    }
    // 項目そのものが無い（別のツールの項目しか無い）
    assert_eq!(summarize("Bash", &json!({ "file_path": "a.txt" })), "");
    assert_eq!(
        summarize("NotebookEdit", &json!({ "file_path": "a.ipynb" })),
        ""
    );
    assert_eq!(summarize("WebFetch", &json!({ "query": "q" })), "");
    assert_eq!(summarize("Agent", &json!({ "prompt": "p" })), "");
    assert_eq!(summarize("Bash", &json!({})), "");
    // tool_input がオブジェクトでない
    for not_obj in [Value::Null, json!("ls"), json!(["command", "ls"]), json!(3)] {
        assert_eq!(summarize("Bash", &not_obj), "", "tool_input = {not_obj:?}");
    }
    // 空文字の値は空文字
    assert_eq!(summarize("Bash", &json!({ "command": "" })), "");
}

// 要件 2: 取り出した値は sanitize_line(value, SUMMARY_MAX_CHARS) を通す
#[test]
fn summarize_sanitizes_and_truncates() {
    let cmd = "git\n  status\t--short\r\n";
    assert_eq!(
        summarize("Bash", &json!({ "command": cmd })),
        "git status --short"
    );
    assert_eq!(
        summarize("Bash", &json!({ "command": cmd })),
        sanitize_line(cmd, SUMMARY_MAX_CHARS)
    );

    let exact = "x".repeat(SUMMARY_MAX_CHARS);
    assert_eq!(summarize("Bash", &json!({ "command": exact })), exact);

    let over = "x".repeat(SUMMARY_MAX_CHARS + 1);
    let got = summarize("Bash", &json!({ "command": over }));
    assert_eq!(got, format!("{}…", "x".repeat(SUMMARY_MAX_CHARS - 1)));
    assert_eq!(got.chars().count(), SUMMARY_MAX_CHARS);

    let emoji = "😀".repeat(200);
    let got = summarize("Write", &json!({ "file_path": emoji }));
    assert_eq!(got, format!("{}…", "😀".repeat(SUMMARY_MAX_CHARS - 1)));
    assert_eq!(got, sanitize_line(&emoji, SUMMARY_MAX_CHARS));

    let ws = "   \n\t  ";
    assert_eq!(summarize("Grep", &json!({ "pattern": ws })), "");
}

// ---------------------------------------------------------------------------
// 3. sanitize_line
// ---------------------------------------------------------------------------

// 要件 3: 例 sanitize_line("a bcd", 3) は "a …"（切り詰め後に trim しない）
#[test]
fn sanitize_example_no_retrim_after_truncation() {
    assert_eq!(sanitize_line("a bcd", 3), "a …");
    assert_eq!(sanitize_line("ab cd", 4), "ab …");
    assert_eq!(sanitize_line("a\n\nbcd", 3), "a …");
}

// 要件 3: 制御文字（改行・タブを含む）は空白へ、空白の連続は 1 つに、前後は除く
#[test]
fn sanitize_control_and_whitespace() {
    let cases = [
        ("a\nb", "a b"),
        ("a\r\nb", "a b"),
        ("a\tb", "a b"),
        ("a\n\n\t \r\nb", "a b"),
        ("a    b", "a b"),
        ("  a  ", "a"),
        ("\n\ta\t\n", "a"),
        ("a\u{0}b", "a b"),
        ("a\u{7}\u{1b}b", "a b"),
        ("a\u{7f}b", "a b"),
        ("a\u{85}b", "a b"),
        ("a\u{9f}b", "a b"),
        ("\u{1b}[31mred\u{1b}[0m", "[31mred [0m"),
        ("a\u{3000}\u{3000}b", "a b"),
        ("a \u{a0} b", "a b"),
        ("a\u{2028}b", "a b"),
        ("x\u{0}", "x"),
        ("\u{0}\u{0}", ""),
        ("", ""),
        ("   ", ""),
        ("\r\n\t", ""),
        ("one two", "one two"),
    ];
    for (input, expected) in cases {
        assert_eq!(sanitize_line(input, 100), expected, "input = {input:?}");
    }
}

// 要件 3: 1 つだけの全角空白・NBSP も半角空白 1 つになる（解釈は questions.md Q1）
#[test]
fn sanitize_single_unicode_whitespace_becomes_ascii_space() {
    assert_eq!(sanitize_line("a\u{3000}b", 10), "a b");
    assert_eq!(sanitize_line("a\u{a0}b", 10), "a b");
}

// 要件 3: ゼロ幅文字は制御文字でも空白でもないので残り、1 文字と数える
#[test]
fn sanitize_keeps_zero_width_chars() {
    assert_eq!(sanitize_line("a\u{200B}b", 10), "a\u{200B}b");
    assert_eq!(sanitize_line("\u{200B}a\u{200B}", 10), "\u{200B}a\u{200B}");
    assert_eq!(sanitize_line("\u{FEFF}x", 10), "\u{FEFF}x");
    assert_eq!(sanitize_line(" \u{200D} ", 10), "\u{200D}");
    // 数える: 3 文字ちょうどなら切らない、4 文字なら切る
    assert_eq!(sanitize_line("a\u{200B}b", 3), "a\u{200B}b");
    assert_eq!(sanitize_line("a\u{200B}\u{200B}b", 3), "a\u{200B}…");
}

// 要件 3: ちょうど max_chars ならそのまま、超えたら先頭 max_chars-1 文字＋…
#[test]
fn sanitize_truncation_boundary() {
    assert_eq!(sanitize_line("abc", 3), "abc");
    assert_eq!(sanitize_line("abcd", 3), "ab…");
    assert_eq!(sanitize_line("abcdefgh", 5), "abcd…");
    assert_eq!(sanitize_line("ab", 3), "ab");
    // 文字数はまとめ・trim の後で数える
    assert_eq!(sanitize_line("a      b", 3), "a b");
    assert_eq!(sanitize_line("   abc   ", 3), "abc");
    assert_eq!(sanitize_line("\nabc\n", 3), "abc");
    // 大きな max_chars でも溢れない
    assert_eq!(sanitize_line("abc", usize::MAX), "abc");
}

// 要件 3: max_chars == 0 は空文字
#[test]
fn sanitize_max_chars_zero() {
    assert_eq!(sanitize_line("abc", 0), "");
    assert_eq!(sanitize_line("a", 0), "");
    assert_eq!(sanitize_line("", 0), "");
    assert_eq!(sanitize_line("   ", 0), "");
    assert_eq!(sanitize_line("😀", 0), "");
}

// 要件 3: max_chars == 1 は 2 文字以上なら "…"、1 文字以下ならそのまま
#[test]
fn sanitize_max_chars_one() {
    assert_eq!(sanitize_line("ab", 1), "…");
    assert_eq!(sanitize_line("abc", 1), "…");
    assert_eq!(sanitize_line("😀😀", 1), "…");
    assert_eq!(sanitize_line("a", 1), "a");
    assert_eq!(sanitize_line("  a\n", 1), "a");
    assert_eq!(sanitize_line("", 1), "");
    assert_eq!(sanitize_line(" \t ", 1), "");
    assert_eq!(sanitize_line("…", 1), "…");
}

// 要件 3: 文字数は Unicode のスカラー値で数える（バイト数ではない）・絵文字の合字は途中で切れてよい
#[test]
fn sanitize_counts_scalar_values() {
    assert_eq!(sanitize_line("日本語", 3), "日本語");
    assert_eq!(sanitize_line("あいうえお", 3), "あい…");
    assert_eq!(sanitize_line("😀😀😀", 3), "😀😀😀");
    assert_eq!(sanitize_line("😀😀😀😀", 3), "😀😀…");
    // 👨‍👩‍👧 は 5 スカラー値（人・ZWJ・人・ZWJ・人）
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    assert_eq!(family.chars().count(), 5);
    assert_eq!(sanitize_line(family, 5), family);
    assert_eq!(sanitize_line(family, 3), "\u{1F468}\u{200D}…");
    assert_eq!(sanitize_line(family, 4), "\u{1F468}\u{200D}\u{1F469}…");
    // 結合文字（e + U+0301）も 2 文字と数える
    assert_eq!(sanitize_line("e\u{301}e\u{301}", 3), "e\u{301}…");
    // 国旗（地域指示子 2 つ）
    assert_eq!(sanitize_line("\u{1F1EF}\u{1F1F5}x", 2), "\u{1F1EF}…");
}

/// 要件 3 の手順をそのまま書いた参照実装（照合用）。
fn reference_sanitize(s: &str, max_chars: usize) -> String {
    let replaced: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let mut collapsed = String::new();
    let mut in_ws = false;
    for c in replaced.chars() {
        if c.is_whitespace() {
            if !in_ws {
                collapsed.push(' ');
            }
            in_ws = true;
        } else {
            collapsed.push(c);
            in_ws = false;
        }
    }
    let trimmed = collapsed.trim_matches(' ');
    let n = trimmed.chars().count();
    if n <= max_chars {
        return trimmed.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let mut out: String = trimmed.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

// 要件 3: いろいろな入力 × max_chars 0..=12 で、手順どおりの結果と一致し、文字数が max_chars 以下
#[test]
fn sanitize_matches_spec_procedure_on_corpus() {
    let corpus = [
        "",
        " ",
        "a",
        "ab",
        "a b",
        "a bcd",
        "  lead",
        "trail  ",
        "x\ny\r\nz",
        "tab\tsep\tvalues",
        "\u{0}\u{1}\u{2}abc\u{7f}",
        "a\u{3000}b\u{a0}c\u{2003}d",
        "日本語のテキストです",
        "😀 😃 😄 😁 😆",
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}",
        "a\u{200B}b\u{200D}c\u{FEFF}d",
        "word1   word2\n\n\nword3\t\t\tword4",
        "…already…",
        "\u{85}\u{2028}\u{2029}x",
        "     a     b     c     ",
    ];
    for input in corpus {
        for max in 0..=12 {
            let got = sanitize_line(input, max);
            assert_eq!(
                got,
                reference_sanitize(input, max),
                "input = {input:?}, max = {max}"
            );
            assert!(got.chars().count() <= max, "input = {input:?}, max = {max}");
            assert!(!got.chars().any(char::is_control), "input = {input:?}");
            assert!(!got.contains("  "), "input = {input:?}, max = {max}");
            assert!(!got.starts_with(' '), "input = {input:?}, max = {max}");
        }
    }
}

// ---------------------------------------------------------------------------
// 4. to_agent_event
// ---------------------------------------------------------------------------

// 要件 4: HookInput → AgentEvent の対応表
#[test]
fn to_agent_event_mapping() {
    let s = || st("sess");
    let cases = [
        (
            HookInput::Prompt { session: s() },
            Some(AgentEvent::Prompt { session: s() }),
        ),
        (
            HookInput::ToolStart {
                session: s(),
                tool: st("Bash"),
            },
            Some(AgentEvent::ToolStart { session: s() }),
        ),
        (
            HookInput::PermissionRequest {
                session: s(),
                tool: st("Bash"),
                summary: st("ls"),
            },
            Some(AgentEvent::NeedsInput { session: s() }),
        ),
        (
            HookInput::Notification {
                session: s(),
                kind: NotificationKind::PermissionPrompt,
            },
            Some(AgentEvent::NeedsInput { session: s() }),
        ),
        (
            HookInput::Notification {
                session: s(),
                kind: NotificationKind::ElicitationDialog,
            },
            Some(AgentEvent::NeedsInput { session: s() }),
        ),
        (
            HookInput::Notification {
                session: s(),
                kind: NotificationKind::IdlePrompt,
            },
            None,
        ),
        (
            HookInput::Notification {
                session: s(),
                kind: NotificationKind::Other(String::new()),
            },
            None,
        ),
        (
            HookInput::Notification {
                session: s(),
                kind: NotificationKind::Other(st("permission_prompt_like")),
            },
            None,
        ),
        (
            HookInput::Stop { session: s() },
            Some(AgentEvent::Done { session: s() }),
        ),
        (
            HookInput::StopFailure {
                session: s(),
                error: st("rate_limit"),
            },
            Some(AgentEvent::Error { session: s() }),
        ),
        (
            HookInput::Other {
                session: s(),
                event: st("SessionStart"),
            },
            None,
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(to_agent_event(&input), expected, "input = {input:?}");
    }
}

// 要件 4: session はそのまま引き継ぐ（空文字・改行入りでも）
#[test]
fn to_agent_event_keeps_session() {
    for session in ["", "a\nb", "セッション"] {
        assert_eq!(
            to_agent_event(&HookInput::Stop {
                session: st(session)
            }),
            Some(AgentEvent::Done {
                session: st(session)
            })
        );
    }
}

// 要件 1+4: parse から to_agent_event までつないだ結果
#[test]
fn parse_then_to_agent_event() {
    let r = parse_value(&json!({
        "hook_event_name": "Notification",
        "session_id": "abc",
        "notification_type": "elicitation_dialog"
    }))
    .unwrap();
    assert_eq!(
        to_agent_event(&r),
        Some(AgentEvent::NeedsInput { session: st("abc") })
    );
}

// ---------------------------------------------------------------------------
// 5. permission_output
// ---------------------------------------------------------------------------

fn assert_single_line(out: &str) {
    assert!(!out.contains('\n'), "改行を含む: {out:?}");
    assert!(!out.contains('\r'), "CR を含む: {out:?}");
}

// 要件 5: Allow
#[test]
fn permission_output_allow() {
    let out = permission_output(&Decision::Allow);
    assert_single_line(&out);
    let v: Value = serde_json::from_str(&out).expect("JSON として読める");
    assert_eq!(
        v,
        json!({
            "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": { "behavior": "allow" }
            }
        })
    );
}

// 要件 5: Deny { message }（エスケープが正しい・1 行）
#[test]
fn permission_output_deny_escapes_message() {
    let messages = [
        String::new(),
        st("Denied from pet"),
        st("quote \" backslash \\ slash /"),
        st("line1\nline2\r\nline3\ttab"),
        st("ctl \u{0}\u{1}\u{1f}\u{7f} end"),
        st("日本語と絵文字😀と\u{2028}\u{2029}"),
        st("</script><!-- {\"a\":1} -->"),
        "長".repeat(2_000),
    ];
    for message in messages {
        let out = permission_output(&Decision::Deny {
            message: message.clone(),
        });
        assert_single_line(&out);
        let v: Value = serde_json::from_str(&out).expect("JSON として読める");
        assert_eq!(
            v,
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "PermissionRequest",
                    "decision": { "behavior": "deny", "message": message }
                }
            }),
            "message = {message:?}"
        );
    }
}

// 要件 5: Ask は {} と完全一致
#[test]
fn permission_output_ask_is_empty_object() {
    assert_eq!(permission_output(&Decision::Ask), "{}");
}
