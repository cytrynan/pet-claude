//! `pet_core::chat` の受け入れテスト（テスト先行）。
//!
//! 原本: `.steering/20261021-phase3-chat/core-requirements.md` の識別子契約と 1〜9 節・追補・追補 2。公開 API だけを使う。
//! 要求書で決まっていない点（questions.md の Q1〜）に依存する入力は避けている。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use pet_core::chat::{
    AuthMode, BUBBLE_HIDDEN, BUBBLE_MAX_CHARS, CREDENTIAL_TARGET, ChatError, ChatOutcome,
    FAILURE_MAX_CHARS, MAX_INPUT_CHARS, MODEL, PANEL_MAX_CHARS, TIMEOUT_SECS, auth_status_label,
    bubble_text, child_args, child_env, find_claude, input_text, is_session_id, mask_secrets,
    panel_text, parse_output,
};
use serde_json::{Value, json};

const SID: &str = "0f8c2a1e-3b4d-4e5f-9a6b-7c8d9e0f1a2b";
const SID_UPPER: &str = "0F8C2A1E-3B4D-4E5F-9A6B-7C8D9E0F1A2B";

const EMPTY_ANSWER: &str = "（答えが空でした）";
const MOVED_TO_PANEL: &str = "（答えはチャット欄に出しました）";

fn st(x: &str) -> String {
    x.to_string()
}

fn env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(k, v)| (st(k), st(v))).collect()
}

fn out_bytes(v: &Value) -> Vec<u8> {
    v.to_string().into_bytes()
}

/// 吹き出しの不変条件（§6）: 1〜140 字・制御文字なし。
fn assert_bubble_invariant(s: &str) {
    let n = s.chars().count();
    assert!(
        (1..=BUBBLE_MAX_CHARS).contains(&n),
        "吹き出しの長さが 1〜140 字でない: {n} 字 {s:?}"
    );
    assert!(
        !s.chars().any(|c| c.is_control()),
        "吹き出しに制御文字が入っている: {s:?}"
    );
}

/// `pattern` を繰り返して、ちょうど `n` 字の文字列を作る。
fn cycle(pattern: &str, n: usize) -> String {
    pattern.chars().cycle().take(n).collect()
}

/// `exists` として渡す偽の関数のもと（ある、とするパスの集合）。
fn fs(paths: &[PathBuf]) -> HashSet<PathBuf> {
    paths.iter().cloned().collect()
}

fn npm_exe(d: &Path) -> PathBuf {
    d.join("node_modules")
        .join("@anthropic-ai")
        .join("claude-code")
        .join("bin")
        .join("claude.exe")
}

fn native_exe(h: &Path) -> PathBuf {
    h.join(".local").join("bin").join("claude.exe")
}

fn find(dirs: &[PathBuf], home: Option<&Path>, existing: &HashSet<PathBuf>) -> Option<PathBuf> {
    let f = |p: &Path| existing.contains(p);
    find_claude(dirs, home, &f)
}

// ---------------------------------------------------------------------------
// 識別子契約
// ---------------------------------------------------------------------------

// 要件 §識別子契約: 定数の値
#[test]
fn constants_have_contract_values() {
    assert_eq!(MODEL, "sonnet");
    assert_eq!(TIMEOUT_SECS, 120);
    assert_eq!(BUBBLE_MAX_CHARS, 140);
    assert_eq!(PANEL_MAX_CHARS, 4000);
    assert_eq!(CREDENTIAL_TARGET, "pet-claude/anthropic-api-key");
}

// 要件 追補 2 識別子契約: 足した定数の値（BUBBLE_HIDDEN は §6 の文と同じ）
#[test]
fn addendum2_constants_have_contract_values() {
    assert_eq!(MAX_INPUT_CHARS, 2000);
    assert_eq!(FAILURE_MAX_CHARS, 200);
    assert_eq!(BUBBLE_HIDDEN, "（答えはチャット欄に出しました）");
    assert_eq!(BUBBLE_HIDDEN, MOVED_TO_PANEL);
    assert_eq!(bubble_text("`x`"), BUBBLE_HIDDEN);
}

// ---------------------------------------------------------------------------
// 1. AuthMode
// ---------------------------------------------------------------------------

// 要件 §1: "api_key" だけが ApiKey
#[test]
fn auth_mode_api_key_only_for_exact_value() {
    assert_eq!(AuthMode::from_state(Some("api_key")), AuthMode::ApiKey);
}

// 要件 §1: それ以外（None・subscription・知らない値・大文字違い・空文字・前後空白）はサブスク
#[test]
fn auth_mode_everything_else_is_subscription() {
    assert_eq!(AuthMode::from_state(None), AuthMode::Subscription);
    for v in [
        "subscription",
        "API_KEY",
        "Api_Key",
        "",
        "apikey",
        "api-key",
        " api_key",
        "api_key ",
        "unknown",
    ] {
        assert_eq!(
            AuthMode::from_state(Some(v)),
            AuthMode::Subscription,
            "{v:?} はサブスクになるべき"
        );
    }
}

// 要件 §1: as_str の値と往復
#[test]
fn auth_mode_as_str_and_round_trip() {
    assert_eq!(AuthMode::Subscription.as_str(), "subscription");
    assert_eq!(AuthMode::ApiKey.as_str(), "api_key");
    for m in [AuthMode::Subscription, AuthMode::ApiKey] {
        assert_eq!(AuthMode::from_state(Some(m.as_str())), m);
    }
}

// ---------------------------------------------------------------------------
// 2. is_session_id
// ---------------------------------------------------------------------------

// 要件 §2: 8-4-4-4-12 の 16 進（小文字・大文字・混在）は true
#[test]
fn session_id_accepts_uuid_forms() {
    assert!(is_session_id(SID));
    assert!(is_session_id(SID_UPPER));
    assert!(is_session_id("0f8C2a1E-3b4D-4e5F-9A6b-7c8D9e0F1a2B"));
    assert!(is_session_id("00000000-0000-0000-0000-000000000000"));
}

// 要件 §2: 長さ違い（35・37 字・ハイフン無しの 32 字）・空文字は false
#[test]
fn session_id_rejects_wrong_length() {
    assert!(!is_session_id(""));
    assert!(!is_session_id(&SID[..35]));
    assert!(!is_session_id(&format!("{SID}0")));
    assert!(!is_session_id(&format!("{SID}-")));
    assert!(!is_session_id("0f8c2a1e3b4d4e5f9a6b7c8d9e0f1a2b"));
}

// 要件 §2: 前後の空白・改行・波括弧は false
#[test]
fn session_id_rejects_whitespace_and_braces() {
    assert!(!is_session_id(&format!(" {SID}")));
    assert!(!is_session_id(&format!("{SID} ")));
    assert!(!is_session_id(&format!("{SID}\n")));
    assert!(!is_session_id(&format!("{{{SID}}}")));
}

// 要件 §2: ハイフンの位置違い（36 字のまま）は false
#[test]
fn session_id_rejects_misplaced_hyphens() {
    // 7-5-4-4-12
    assert!(!is_session_id("0f8c2a1-e3b4d-4e5f-9a6b-7c8d9e0f1a2b"));
    // 8-4-4-4-12 の区切りが 1 つ空白
    assert!(!is_session_id("0f8c2a1e 3b4d-4e5f-9a6b-7c8d9e0f1a2b"));
    // 区切りの位置にハイフンの代わりに 16 進、別の位置にハイフン（36 字・ハイフン 4 つ）
    assert!(!is_session_id("0f8c2a1e3-b4d-4e5f-9a6b-7c8d9e0f1a2b"));
    // ハイフンだけ 36 字
    assert!(!is_session_id(&"-".repeat(36)));
}

// 要件 §2: 16 進でない文字（g・全角数字・注入の試み）は false
#[test]
fn session_id_rejects_non_hex() {
    assert!(!is_session_id("0f8c2a1g-3b4d-4e5f-9a6b-7c8d9e0f1a2b"));
    assert!(!is_session_id("0f8c2a1e-3b4d-4e5f-9a6b-7c8d9e0f1a2Z"));
    // 全角の「０」で 36 字（バイト数は 36 でない）
    assert!(!is_session_id("０f8c2a1e-3b4d-4e5f-9a6b-7c8d9e0f1a2b"));
    // 36 バイトちょうどだが 35 字（「é」が 2 バイト）
    let tricky = "0f8c2a1e-3b4d-4e5f-9a6b-7c8d9e0f1aé";
    assert_eq!(tricky.len(), 36);
    assert!(!is_session_id(tricky));
    assert!(!is_session_id("0f8c2a1e-3b4d-4e5f-9a6b-7c8d9e0f;rm "));
}

// ---------------------------------------------------------------------------
// 3. child_args
// ---------------------------------------------------------------------------

fn base_args(file: &str) -> Vec<String> {
    [
        "-p",
        "--output-format",
        "json",
        "--safe-mode",
        "--model",
        "sonnet",
        "--tools",
        "",
        "--disallowedTools",
        "mcp__*",
        "--strict-mcp-config",
        "--settings",
        r#"{"disableAllHooks":true}"#,
        "--disable-slash-commands",
        "--permission-prompts",
        "none",
        "--max-turns",
        "1",
        "--system-prompt-file",
        file,
    ]
    .iter()
    .map(|s| st(s))
    .collect()
}

// 要件 §3: resume なしの並びが完全一致（--tools の次は空文字の要素 1 つ）
#[test]
fn child_args_without_resume_exact() {
    let file = r"C:\pet\system.md";
    let got = child_args(file, None);
    assert_eq!(got, base_args(file));
    let i = got.iter().position(|a| a == "--tools").unwrap();
    assert_eq!(got[i + 1], "");
    assert!(!got.iter().any(|a| a == "--resume"));
}

// 要件 §3: 有効な id なら末尾に --resume <id> の 2 要素
#[test]
fn child_args_with_valid_resume_appends_two() {
    let file = r"C:\pet\system.md";
    let mut want = base_args(file);
    want.push(st("--resume"));
    want.push(st(SID));
    assert_eq!(child_args(file, Some(SID)), want);

    let mut want_upper = base_args(file);
    want_upper.push(st("--resume"));
    want_upper.push(st(SID_UPPER));
    assert_eq!(child_args(file, Some(SID_UPPER)), want_upper);
}

// 要件 §3: is_session_id を満たさない resume は付けない（外から来た値を引数に混ぜない）
#[test]
fn child_args_ignores_invalid_resume() {
    let file = r"C:\pet\system.md";
    for bad in [
        "",
        "abc",
        "--dangerously-skip-permissions",
        &format!(" {SID}"),
        &format!("{SID} --tools Bash"),
        &SID[..35],
    ] {
        let got = child_args(file, Some(bad));
        assert_eq!(got, base_args(file), "resume={bad:?} は無視されるべき");
    }
}

// 要件 §3: 空白を含む system_prompt_file は 1 要素のまま
#[test]
fn child_args_keeps_spaced_path_as_one_element() {
    let file = r"C:\Users\K San\AppData\Roaming\pet claude\system prompt.md";
    let got = child_args(file, None);
    assert_eq!(got.len(), 20);
    assert_eq!(got.last().unwrap(), file);
    let i = got
        .iter()
        .position(|a| a == "--system-prompt-file")
        .unwrap();
    assert_eq!(got[i + 1], file);
}

// ---------------------------------------------------------------------------
// 4. child_env
// ---------------------------------------------------------------------------

const STRIPPED: [&str; 8] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    "ANTHROPIC_PROFILE",
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
];

/// 消すべきもの（大文字小文字ばらばら）と、似ているが残すものを混ぜた親の環境。
fn mixed_parent() -> Vec<(String, String)> {
    env(&[
        ("PATH", r"C:\Windows;C:\bin"),
        ("anthropic_api_key", "sk-ant-old"),
        ("ANTHROPIC_API_KEY_OLD", "keep1"),
        ("Anthropic_Auth_Token", "tok"),
        ("MY_ANTHROPIC_API_KEY", "keep2"),
        ("CLAUDE_CODE_OAUTH_TOKEN", "keep3"),
        ("claudecode", "1"),
        ("CLAUDE_CODE_ENTRYPOINT", "cli"),
        ("Claude_Code_Use_Bedrock", "1"),
        ("CLAUDE_CODE_USE_VERTEX", "1"),
        ("claude_code_use_foundry", "1"),
        ("ANTHROPIC_PROFILE", "work"),
        ("PATH", r"C:\dup"),
        ("ANTHROPIC_API_KEY", "sk-ant-old2"),
        ("USERPROFILE", r"C:\Users\K"),
    ])
}

fn mixed_parent_kept() -> Vec<(String, String)> {
    env(&[
        ("PATH", r"C:\Windows;C:\bin"),
        ("ANTHROPIC_API_KEY_OLD", "keep1"),
        ("MY_ANTHROPIC_API_KEY", "keep2"),
        ("CLAUDE_CODE_OAUTH_TOKEN", "keep3"),
        ("PATH", r"C:\dup"),
        ("USERPROFILE", r"C:\Users\K"),
    ])
}

fn count_api_key(v: &[(String, String)]) -> usize {
    v.iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("ANTHROPIC_API_KEY"))
        .count()
}

// 要件 §4: 8 つの名前は大文字・小文字・混在のどれでも消える（1 つずつ）
#[test]
fn child_env_strips_each_name_case_insensitively() {
    for name in STRIPPED {
        let lower = name.to_ascii_lowercase();
        let mut mixed: String = String::new();
        for (i, c) in name.chars().enumerate() {
            mixed.push(if i % 2 == 0 {
                c.to_ascii_lowercase()
            } else {
                c
            });
        }
        for variant in [name.to_string(), lower, mixed] {
            let parent = vec![
                (st("A"), st("1")),
                (variant.clone(), st("secret")),
                (st("B"), st("2")),
            ];
            let got = child_env(AuthMode::Subscription, &parent, None).unwrap();
            assert_eq!(
                got,
                env(&[("A", "1"), ("B", "2")]),
                "{variant:?} が消えていない"
            );
        }
    }
}

// 要件 §4: 似た名前は残し、残すものは元の順と値のまま（Subscription）
#[test]
fn child_env_subscription_keeps_lookalikes_in_order() {
    let got = child_env(AuthMode::Subscription, &mixed_parent(), None).unwrap();
    assert_eq!(got, mixed_parent_kept());
    assert_eq!(count_api_key(&got), 0);
}

// 要件 §4: Subscription では api_key が Some でも子に入れない
#[test]
fn child_env_subscription_ignores_api_key() {
    let got = child_env(AuthMode::Subscription, &mixed_parent(), Some("sk-ant-new")).unwrap();
    assert_eq!(got, mixed_parent_kept());
    assert_eq!(count_api_key(&got), 0);
    assert!(!got.iter().any(|(_, v)| v.contains("sk-ant-new")));
}

// 要件 §4: ApiKey は取り除いたうえで末尾に前後空白を落とした鍵を 1 つ
#[test]
fn child_env_api_key_appends_trimmed_key_once() {
    let got = child_env(AuthMode::ApiKey, &mixed_parent(), Some("  sk-ant-new\r\n")).unwrap();
    let mut want = mixed_parent_kept();
    want.push((st("ANTHROPIC_API_KEY"), st("sk-ant-new")));
    assert_eq!(got, want);
    assert_eq!(count_api_key(&got), 1);
}

// 要件 §4: ApiKey で親が空でも鍵は 1 つ
#[test]
fn child_env_api_key_with_empty_parent() {
    let got = child_env(AuthMode::ApiKey, &[], Some("k")).unwrap();
    assert_eq!(got, env(&[("ANTHROPIC_API_KEY", "k")]));
}

// 要件 §4: ApiKey で鍵が None・空・空白だけなら MissingKey
#[test]
fn child_env_api_key_missing() {
    for key in [None, Some(""), Some("   "), Some("\t\r\n ")] {
        assert_eq!(
            child_env(AuthMode::ApiKey, &mixed_parent(), key),
            Err(ChatError::MissingKey),
            "key={key:?}"
        );
    }
}

// 要件 §4: Subscription は鍵が None でも空でも Ok
#[test]
fn child_env_subscription_never_missing_key() {
    for key in [None, Some(""), Some("  ")] {
        assert!(child_env(AuthMode::Subscription, &mixed_parent(), key).is_ok());
    }
}

// ---------------------------------------------------------------------------
// 5. parse_output
// ---------------------------------------------------------------------------

// 要件 §5: 成功の JSON（前後に改行）→ Answer。text は生の文字列（改行も整形しない）
#[test]
fn parse_output_answer_raw_text_with_surrounding_newlines() {
    let raw = "一行目\n二行目 `code` https://example.com";
    let mut stdout = b"\r\n\n  ".to_vec();
    stdout.extend(out_bytes(&json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "result": raw,
        "session_id": SID
    })));
    stdout.extend(b"\n\n");
    assert_eq!(
        parse_output(Some(0), &stdout, b""),
        ChatOutcome::Answer {
            text: st(raw),
            session_id: Some(st(SID))
        }
    );
}

// 要件 §5: is_error が無い・false のどちらでも Answer
#[test]
fn parse_output_answer_without_is_error() {
    let stdout = out_bytes(&json!({"type": "result", "result": "はい", "session_id": SID}));
    assert_eq!(
        parse_output(Some(0), &stdout, b""),
        ChatOutcome::Answer {
            text: st("はい"),
            session_id: Some(st(SID))
        }
    );
}

// 要件 §5: session_id は is_session_id を満たす文字列の時だけ Some
#[test]
fn parse_output_session_id_validated() {
    for sid in [
        json!("abc"),
        json!(format!(" {SID}")),
        json!(42),
        json!(null),
        json!([SID]),
    ] {
        let stdout = out_bytes(&json!({"type": "result", "result": "はい", "session_id": sid}));
        assert_eq!(
            parse_output(Some(0), &stdout, b""),
            ChatOutcome::Answer {
                text: st("はい"),
                session_id: None
            },
            "session_id={sid}"
        );
    }
    // session_id が無い
    let stdout = out_bytes(&json!({"type": "result", "result": "はい"}));
    assert_eq!(
        parse_output(Some(0), &stdout, b""),
        ChatOutcome::Answer {
            text: st("はい"),
            session_id: None
        }
    );
}

// 要件 §5: 終了コード 1 でも is_error: true の JSON が読めたら Failed（reason は bubble_text、session_id も拾う）
#[test]
fn parse_output_exit_1_is_error_json() {
    let stdout = out_bytes(&json!({
        "type": "result",
        "subtype": "success",
        "is_error": true,
        "result": "Credit balance is too low",
        "session_id": SID
    }));
    assert_eq!(
        parse_output(Some(1), &stdout, b"some stderr noise\n"),
        ChatOutcome::Failed {
            reason: st("Credit balance is too low"),
            session_id: Some(st(SID))
        }
    );
}

// 要件 §5 追補 2 D: is_error の reason は bubble_text でなく 1 行の失敗の文（改行が消え、200 字で切る）
#[test]
fn parse_output_is_error_reason_is_one_line_of_200() {
    let long = format!("Invalid API key\n{}", cycle("エラー。", 300));
    let stdout = out_bytes(&json!({"type": "result", "is_error": true, "result": long}));
    match parse_output(Some(1), &stdout, b"") {
        ChatOutcome::Failed { reason, session_id } => {
            assert_eq!(session_id, None);
            let want = format!("Invalid API key {}…", cycle("エラー。", 183));
            assert_eq!(reason, want);
            assert_eq!(reason.chars().count(), FAILURE_MAX_CHARS);
            assert!(!reason.contains('\n'));
        }
        other => panic!("Failed のはず: {other:?}"),
    }
}

// 要件 §5: is_error で result が無い・文字列でない・空 → 決まった文
#[test]
fn parse_output_is_error_without_usable_result() {
    for result in [None, Some(json!(12)), Some(json!(null)), Some(json!(""))] {
        let mut m = json!({"type": "result", "is_error": true, "session_id": SID});
        if let Some(r) = result.clone() {
            m["result"] = r;
        }
        assert_eq!(
            parse_output(Some(1), &out_bytes(&m), b""),
            ChatOutcome::Failed {
                reason: st("Claude Code がエラーを返しました"),
                session_id: Some(st(SID))
            },
            "result={result:?}"
        );
    }
}

// 要件 §5: 成功扱いで result が無い・空・文字列でない → 「答えが空でした」
#[test]
fn parse_output_empty_answer() {
    for result in [
        None,
        Some(json!("")),
        Some(json!(5)),
        Some(json!({"a": 1})),
        Some(json!(null)),
    ] {
        let mut m = json!({"type": "result", "is_error": false, "session_id": SID});
        if let Some(r) = result.clone() {
            m["result"] = r;
        }
        assert_eq!(
            parse_output(Some(0), &out_bytes(&m), b""),
            ChatOutcome::Failed {
                reason: st("答えが空でした"),
                session_id: Some(st(SID))
            },
            "result={result:?}"
        );
    }
}

// 要件 §5: 終了コードが 0 以外・None でも、読めた成功 JSON は Answer
#[test]
fn parse_output_nonzero_exit_with_success_json() {
    let stdout = out_bytes(&json!({"type": "result", "is_error": false, "result": "ok"}));
    for code in [Some(1), Some(-1), None] {
        assert_eq!(
            parse_output(code, &stdout, b"warning\n"),
            ChatOutcome::Answer {
                text: st("ok"),
                session_id: None
            },
            "exit={code:?}"
        );
    }
}

// 要件 §5: JSON でない出力 → 標準エラーの最初の空白以外を含む行（session_id は None）
#[test]
fn parse_output_not_json_uses_first_nonblank_stderr_line() {
    let stderr = b"\n   \n\t\r\nError: Invalid model name\r\nsecond line\n";
    assert_eq!(
        parse_output(Some(1), b"Hello, I am not JSON", stderr),
        ChatOutcome::Failed {
            reason: st("Error: Invalid model name"),
            session_id: None
        }
    );
}

// 要件 §5 追補 2 D: 標準エラーの理由はパスを残す（bubble_text で置き換えない。吹き出しで隠すのはアプリ側）
#[test]
fn parse_output_stderr_reason_keeps_path() {
    let stderr = br"Error: cannot open C:\Users\K\secret.txt";
    assert_eq!(
        parse_output(Some(1), b"", stderr),
        ChatOutcome::Failed {
            reason: st(r"Error: cannot open C:\Users\K\secret.txt"),
            session_id: None
        }
    );
    // 吹き出しに出すときに bubble_text を通せば隠れる
    if let ChatOutcome::Failed { reason, .. } = parse_output(Some(1), b"", stderr) {
        assert_eq!(bubble_text(&reason), BUBBLE_HIDDEN);
    }
}

// 要件 §5: 不正な UTF-8 の標準エラーは置き換えて読む
#[test]
fn parse_output_invalid_utf8_stderr() {
    let stderr = b"bad \xff\xfe bytes\nnext";
    match parse_output(Some(2), b"", stderr) {
        ChatOutcome::Failed { reason, session_id } => {
            assert_eq!(session_id, None);
            assert!(reason.starts_with("bad "), "{reason:?}");
            assert!(reason.ends_with(" bytes"), "{reason:?}");
            assert!(reason.contains('\u{FFFD}'), "{reason:?}");
            assert!(!reason.contains("next"));
        }
        other => panic!("Failed のはず: {other:?}"),
    }
}

// 要件 §5: 標準エラーが空（空白だけ）なら終了コードつきの文。None は「なし」
#[test]
fn parse_output_no_stderr_reports_exit_code() {
    assert_eq!(
        parse_output(Some(3), b"garbage", b""),
        ChatOutcome::Failed {
            reason: st("Claude Code が答えませんでした（終了コード 3）"),
            session_id: None
        }
    );
    assert_eq!(
        parse_output(Some(0), b"", b"  \n\t\r\n  "),
        ChatOutcome::Failed {
            reason: st("Claude Code が答えませんでした（終了コード 0）"),
            session_id: None
        }
    );
    assert_eq!(
        parse_output(None, b"", b""),
        ChatOutcome::Failed {
            reason: st("Claude Code が答えませんでした（終了コード なし）"),
            session_id: None
        }
    );
}

// 要件 §5: オブジェクトでない JSON・type が result でない・type が無い → 読めない扱い（session_id も拾わない）
#[test]
fn parse_output_wrong_shape_json_falls_back() {
    let cases = vec![
        out_bytes(&json!([{"type": "result", "result": "x", "session_id": SID}])),
        out_bytes(&json!("result")),
        out_bytes(&json!({"type": "assistant", "result": "x", "session_id": SID})),
        out_bytes(&json!({"type": "Result", "result": "x", "session_id": SID})),
        out_bytes(&json!({"result": "x", "session_id": SID})),
        // 2 つ目のオブジェクトが続く（オブジェクト 1 つではない）
        format!(
            "{}\n{}",
            json!({"type": "result", "result": "x"}),
            json!({"type": "result", "result": "y"})
        )
        .into_bytes(),
    ];
    for stdout in cases {
        assert_eq!(
            parse_output(Some(0), &stdout, b"boom\n"),
            ChatOutcome::Failed {
                reason: st("boom"),
                session_id: None
            },
            "stdout={:?}",
            String::from_utf8_lossy(&stdout)
        );
    }
}

// 要件 §5: 標準出力が不正な UTF-8 なら読めない扱い
#[test]
fn parse_output_invalid_utf8_stdout_falls_back() {
    let mut stdout = br#"{"type":"result","result":""#.to_vec();
    stdout.extend(b"\xff\xfe");
    stdout.extend(br#""}"#);
    assert_eq!(
        parse_output(Some(0), &stdout, b"fatal\n"),
        ChatOutcome::Failed {
            reason: st("fatal"),
            session_id: None
        }
    );
}

// ---------------------------------------------------------------------------
// 6. bubble_text
// ---------------------------------------------------------------------------

// 要件 §6: 普通の文（句読点・絵文字・% ・単体の : と /）はそのまま
#[test]
fn bubble_text_keeps_ordinary_sentences() {
    for s in [
        "こんにちは！今日もがんばろう 🐉",
        "進み具合は 50% です。次: テスト / 片付け",
        "Hello! It's 3:30 now. Yes/No?",
        "task-list を見直したよ",
        "desk-top と skill-up の話",
        "比率 1:2、A/B テスト",
    ] {
        assert_eq!(bubble_text(s), s, "{s:?} はそのまま出るべき");
    }
}

// 要件 §6: 区切りの無い長い日本語（32 字以上）は秘密と見なさない
#[test]
fn bubble_text_long_japanese_run_is_not_secret() {
    let s = "今日はとても良い天気なので散歩に行きましょうそれから買い物もしてから帰りましょうね";
    assert!(s.chars().count() >= 40);
    assert_eq!(bubble_text(s), s);
}

// 要件 §6: 改行・CR・タブ・その他の制御文字は 1 行にまとめる（sanitize_line）
#[test]
fn bubble_text_single_line() {
    assert_eq!(bubble_text("  a\nb\r\nc\td\u{0007}e  "), "a b c d e");
    assert_eq!(bubble_text("一行目\n\n\n二行目"), "一行目 二行目");
    assert_bubble_invariant(&bubble_text("x\u{001b}[31my"));
}

// 要件 §6: 空・空白だけ・制御文字だけ → 「（答えが空でした）」
#[test]
fn bubble_text_empty() {
    for s in ["", "   ", "\n\r\t", "\u{0000}\u{0007}"] {
        assert_eq!(bubble_text(s), EMPTY_ANSWER, "{s:?}");
    }
}

// 要件 §6: 140 字はそのまま、141 字は 139 字＋…（ちょうど 140 字）
#[test]
fn bubble_text_140_141_boundary() {
    let s140 = cycle("あい。", 140);
    assert_eq!(bubble_text(&s140), s140);

    let s141 = cycle("あい。", 141);
    let got = bubble_text(&s141);
    let want = format!("{}…", cycle("あい。", 139));
    assert_eq!(got, want);
    assert_eq!(got.chars().count(), 140);

    let long = cycle("Hi there. ", 1000);
    assert_bubble_invariant(&bubble_text(&long));
    assert!(bubble_text(&long).ends_with('…'));
}

// 要件 §6: URL（:// と www.）は吹き出しに出さない
#[test]
fn bubble_text_hides_urls() {
    for s in [
        "詳しくは https://example.com を見てね",
        "ftp://files.example.org",
        "www.example.com にあるよ",
        "x://y",
    ] {
        assert_eq!(bubble_text(s), MOVED_TO_PANEL, "{s:?}");
    }
}

// 要件 §6: パス（英字+:\・英字+:/・\\・~/・/Users/・/home/・/etc/）は出さない
#[test]
fn bubble_text_hides_paths() {
    for s in [
        r"C:\Users\K を開いた",
        r"ファイルは d:\work\a.txt",
        "保存先 d:/work",
        "Z:/",
        r"\\server\share にある",
        "~/notes を見て",
        "場所は /Users/k/Desktop",
        "/home/k/.bashrc を直した",
        "/etc/hosts を書き換え",
    ] {
        assert_eq!(bubble_text(s), MOVED_TO_PANEL, "{s:?}");
    }
}

// 要件 §6: コード（バッククォート）は出さない
#[test]
fn bubble_text_hides_code() {
    for s in ["`cargo test` を回して", "a ` b", "```"] {
        assert_eq!(bubble_text(s), MOVED_TO_PANEL, "{s:?}");
    }
}

// 要件 §6: sk- で始まる語（直前が英数字でない）は出さない。task- の中の sk- は対象外
#[test]
fn bubble_text_hides_sk_prefix_words() {
    for s in [
        "sk-abc",
        "鍵は sk-ant-xyz です",
        "(sk-1)",
        "鍵:sk-zzz",
        "日本語sk-abc",
    ] {
        assert_eq!(bubble_text(s), MOVED_TO_PANEL, "{s:?}");
    }
    for s in ["task-list", "mask-off", "a1sk-b", "risk-free な案"] {
        assert_eq!(bubble_text(s), s, "{s:?} は誤検出しないこと");
    }
}

// 要件 §6: 英数字・_・- だけが 32 字以上続く並びは出さない。31 字は出す
#[test]
fn bubble_text_long_token_boundary() {
    let t31 = cycle("a_b-", 31);
    let t32 = cycle("a_b-", 32);
    assert_eq!(
        bubble_text(&format!("id は {t31} です")),
        format!("id は {t31} です")
    );
    assert_eq!(bubble_text(&format!("id は {t32} です")), MOVED_TO_PANEL);

    let hex31 = cycle("0123456789abcdef", 31);
    let hex32 = cycle("0123456789abcdef", 32);
    assert_eq!(bubble_text(&hex31), hex31);
    assert_eq!(bubble_text(&hex32), MOVED_TO_PANEL);
    // セッション id（36 字）も秘密らしい並び
    assert_eq!(bubble_text(&format!("session {SID}")), MOVED_TO_PANEL);
    // 空白で区切られていれば数えない
    let spaced = format!("{} {}", cycle("abc", 20), cycle("xyz", 20));
    assert_eq!(bubble_text(&spaced), spaced);
}

// 要件 §6: 判定は sanitize 後の文で行う（改行をまたいでも、結果は単一行）
#[test]
fn bubble_text_detection_after_sanitize() {
    assert_eq!(bubble_text("see\nhttps://example.com\n"), MOVED_TO_PANEL);
    // 制御文字は空白になるので、区切られたトークンは 32 字に達しない
    let a = cycle("abcd", 20);
    let b = cycle("efgh", 20);
    let joined = format!("{a}\u{0007}{b}");
    assert_eq!(bubble_text(&joined), format!("{a} {b}"));
}

// 要件 §6: どんな入力でも 1〜140 字・単一行
#[test]
fn bubble_text_invariant_over_samples() {
    let samples = vec![
        st(""),
        st("\n"),
        cycle("a\n", 999),
        cycle("長い文。", 5000),
        cycle("https://x ", 50),
        st("\u{2028}\u{0085}x\u{000b}y"),
        cycle(" \t", 300),
    ];
    for s in samples {
        assert_bubble_invariant(&bubble_text(&s));
    }
}

// 要件 §6 追補 Q1: 検査は切り詰める前の 1 行で行う（130 字の文の後に sk-ant- で始まる 40 字の鍵）
#[test]
fn bubble_text_secret_after_130_chars_is_hidden() {
    let key = format!("sk-ant-{}", cycle("a1B2c3", 33));
    assert_eq!(key.chars().count(), 40);
    let s = format!("{}{key}", cycle("あい。", 130));
    assert_eq!(bubble_text(&s), MOVED_TO_PANEL);
}

// 要件 §6 追補 Q1: 鍵の sk- が切り詰めでちょうど切れる位置（137 字目から）でも隠す
#[test]
fn bubble_text_sk_cut_by_truncation_is_hidden() {
    // 136 字の文＋空白（137 字目）＋鍵。切り詰め後なら "sk…" しか残らず sk- も 32 字の並びも消える
    let key = format!("sk-ant-{}", cycle("a1B2c3", 33));
    let s = format!("{} {key}", cycle("あい。", 136));
    assert_eq!(bubble_text(&s), MOVED_TO_PANEL);
}

// 要件 §6 追補 Q1: 32 字以上の英数字の並びが 140 字の境界をまたいでも隠す
#[test]
fn bubble_text_long_token_across_boundary_is_hidden() {
    // 並びは 121 字目から 40 字。切り詰め後なら 19 字しか残らない
    let s = format!("{} {}", cycle("あい。", 119), cycle("0123456789abcdef", 40));
    assert_eq!(bubble_text(&s), MOVED_TO_PANEL);
}

// 要件 §6 追補 Q1: 141 字目以降の URL・バッククォート・パスも隠す
#[test]
fn bubble_text_patterns_after_141_chars_are_hidden() {
    let head = cycle("あい。", 150);
    for tail in [
        " https://example.com",
        " www.example.com",
        " `cargo test`",
        r" C:\Users\K",
        " ~/notes",
    ] {
        let s = format!("{head}{tail}");
        assert_eq!(bubble_text(&s), MOVED_TO_PANEL, "{tail:?}");
    }
    // :// が切り詰めの位置（139 字目の後）でちょうど切れる
    let s = format!("{}https://example.com", cycle("あい。", 133));
    assert_eq!(bubble_text(&s), MOVED_TO_PANEL);
}

// 要件 §6 追補 Q1: 検査に通った長い文は従来どおり 139 字＋…
#[test]
fn bubble_text_clean_long_text_still_truncated() {
    let s = format!("{} おわり", cycle("あい。", 300));
    let got = bubble_text(&s);
    assert_eq!(got, format!("{}…", cycle("あい。", 139)));
    assert_bubble_invariant(&got);
}

// 要件 §6 追補 Q3: sk-・www.・:// は ASCII の大文字小文字を無視して隠す
#[test]
fn bubble_text_patterns_case_insensitive() {
    for s in [
        "SK-abc",
        "鍵は SK-ANT-xyz です",
        "Sk-abc",
        "sK-1",
        "(SK-zz)",
        "WWW.example.com を見て",
        "Www.Example.com",
        "HTTPS://x",
        "Ftp://Files",
    ] {
        assert_eq!(bubble_text(s), MOVED_TO_PANEL, "{s:?}");
    }
}

// 要件 §6 追補 Q3: 大文字でも sk- の直前が英数字なら隠さない（語の途中）
#[test]
fn bubble_text_uppercase_sk_inside_word_is_kept() {
    for s in ["RISK-FREE な案", "TASK-LIST", "aSk-b", "MASK-OFF"] {
        assert_eq!(bubble_text(s), s, "{s:?} は誤検出しないこと");
    }
}

// ---------------------------------------------------------------------------
// 7. panel_text
// ---------------------------------------------------------------------------

// 要件 §7: CRLF と単独の CR は LF 1 つに
#[test]
fn panel_text_normalizes_newlines() {
    assert_eq!(panel_text("a\r\nb\rc\nd"), "a\nb\nc\nd");
    assert_eq!(panel_text("a\r\n\r\nb"), "a\n\nb");
}

// 要件 §7: LF 以外の制御文字（タブを含む）は半角空白に。空白の並びは潰さない
#[test]
fn panel_text_control_to_space() {
    assert_eq!(panel_text("a\tb\u{0007}c\u{001b}d"), "a b c d");
    assert_eq!(panel_text("a\t\tb"), "a  b");
    assert_eq!(panel_text("x  y\n\n\nz"), "x  y\n\n\nz");
}

// 要件 §7: 前後の空白・改行は落とし、中の改行は残す
#[test]
fn panel_text_trims_edges_keeps_inner() {
    assert_eq!(
        panel_text(" \r\n\t\n  一行目\n二行目  \n\r\n "),
        "一行目\n二行目"
    );
}

// 要件 §7: URL・パス・コードはチャット欄では消さない（§6 の振り分け先）
#[test]
fn panel_text_keeps_urls_and_code() {
    let s = "see https://example.com and `cargo test` in C:\\work";
    assert_eq!(panel_text(s), s);
}

// 要件 §7: 4000 字はそのまま、4001 字は 3999 字＋…（ちょうど 4000 字）
#[test]
fn panel_text_4000_4001_boundary() {
    // 追補 2 C: ASCII 英数字の長い並びは伏せられるので、境界は「x y 」の繰り返しで見る
    let s4000 = cycle("x y ", 3999) + "z";
    assert_eq!(panel_text(&s4000), s4000);

    let s4001 = cycle("x y ", 4001);
    let got = panel_text(&s4001);
    assert_eq!(got, format!("{}…", cycle("x y ", 3999)));
    assert_eq!(got.chars().count(), PANEL_MAX_CHARS);

    let jp = "あ".repeat(4001);
    let got = panel_text(&jp);
    assert_eq!(got.chars().count(), 4000);
    assert!(got.ends_with("あ…"));
}

// 要件 §7: 前後の空白は数える前に落とす（空白込みで 4000 字を超えても中身が 4000 字なら切らない）
#[test]
fn panel_text_trims_before_counting() {
    let body = "あ".repeat(4000);
    let padded = format!("\n\n   {body}  \r\n");
    assert_eq!(panel_text(&padded), body);
}

// 要件 §7: CRLF は 1 字として数える（4000 行の改行を含む文の長さ）
#[test]
fn panel_text_crlf_counts_as_one() {
    // "a\r\n" × 2000 → "a\n" × 2000 → 末尾の改行を落として 3999 字
    let s = "a\r\n".repeat(2000);
    let got = panel_text(&s);
    assert_eq!(got.chars().count(), 3999);
    assert!(!got.ends_with('…'));
}

// 要件 §7: 空になったら「（答えが空でした）」
#[test]
fn panel_text_empty() {
    for s in ["", " ", "\r\n\r\n", "\t\u{0007}\n"] {
        assert_eq!(panel_text(s), EMPTY_ANSWER, "{s:?}");
    }
}

// ---------------------------------------------------------------------------
// 8. find_claude
// ---------------------------------------------------------------------------

// 要件 §8: PATH の先頭から見て、claude.exe のある最初のフォルダ
#[test]
fn find_claude_first_dir_with_exe_wins() {
    let a = PathBuf::from(r"C:\a");
    let b = PathBuf::from(r"C:\b");
    let c = PathBuf::from(r"C:\c");
    let home = PathBuf::from(r"C:\Users\K");
    let existing = fs(&[
        b.join("claude.exe"),
        c.join("claude.exe"),
        native_exe(&home),
    ]);
    assert_eq!(
        find(&[a, b.clone(), c], Some(&home), &existing),
        Some(b.join("claude.exe"))
    );
}

// 要件 §8: npm 版は claude.cmd と node_modules の exe が両方ある時だけ、exe の方を返す
#[test]
fn find_claude_npm_version_returns_inner_exe() {
    let npm = PathBuf::from(r"C:\Users\K\AppData\Roaming\npm");
    let existing = fs(&[npm.join("claude.cmd"), npm_exe(&npm)]);
    let got = find(std::slice::from_ref(&npm), None, &existing);
    assert_eq!(got, Some(npm_exe(&npm)));
    assert_ne!(got, Some(npm.join("claude.cmd")));
}

// 要件 §8: .cmd だけで隣の exe が無いフォルダは飛ばして次へ
#[test]
fn find_claude_cmd_without_exe_is_skipped() {
    let npm = PathBuf::from(r"C:\npm");
    let next = PathBuf::from(r"C:\next");
    let existing = fs(&[npm.join("claude.cmd"), next.join("claude.exe")]);
    assert_eq!(
        find(&[npm.clone(), next.clone()], None, &existing),
        Some(next.join("claude.exe"))
    );
    // 次も無ければ None（.cmd は返さない）
    let only_cmd = fs(&[npm.join("claude.cmd")]);
    assert_eq!(find(&[npm], None, &only_cmd), None);
}

// 要件 §8: node_modules の exe があっても claude.cmd が無ければ npm 版としては拾わない
#[test]
fn find_claude_npm_exe_without_cmd_is_skipped() {
    let d = PathBuf::from(r"C:\d");
    let home = PathBuf::from(r"C:\Users\K");
    let existing = fs(&[npm_exe(&d), native_exe(&home)]);
    assert_eq!(
        find(std::slice::from_ref(&d), Some(&home), &existing),
        Some(native_exe(&home))
    );
}

// 要件 §8: 同じフォルダに claude.exe と npm 版があれば claude.exe
#[test]
fn find_claude_exe_preferred_over_npm_in_same_dir() {
    let d = PathBuf::from(r"C:\d");
    let existing = fs(&[d.join("claude.exe"), d.join("claude.cmd"), npm_exe(&d)]);
    assert_eq!(
        find(std::slice::from_ref(&d), None, &existing),
        Some(d.join("claude.exe"))
    );
}

// 要件 §8: 先のフォルダの npm 版が、後のフォルダの claude.exe より優先
#[test]
fn find_claude_earlier_npm_beats_later_exe() {
    let a = PathBuf::from(r"C:\a");
    let b = PathBuf::from(r"C:\b");
    let existing = fs(&[a.join("claude.cmd"), npm_exe(&a), b.join("claude.exe")]);
    assert_eq!(find(&[a.clone(), b], None, &existing), Some(npm_exe(&a)));
}

// 要件 §8: 空のパスのフォルダは飛ばす（カレントフォルダの claude.exe を拾わない）
#[test]
fn find_claude_skips_empty_dir() {
    let empty = PathBuf::from("");
    let b = PathBuf::from(r"C:\b");
    // 空のパスに join すると相対の "claude.exe" になる。それが「ある」としても拾わない
    let existing = fs(&[
        empty.join("claude.exe"),
        empty.join("claude.cmd"),
        npm_exe(&empty),
        b.join("claude.exe"),
    ]);
    assert_eq!(
        find(&[empty.clone(), b.clone()], None, &existing),
        Some(b.join("claude.exe"))
    );
    assert_eq!(find(&[empty], None, &existing), None);
}

// 要件 §8: PATH に無ければ home のネイティブ版
#[test]
fn find_claude_falls_back_to_home_native() {
    let a = PathBuf::from(r"C:\a");
    let home = PathBuf::from(r"C:\Users\K");
    let existing = fs(&[native_exe(&home)]);
    assert_eq!(
        find(std::slice::from_ref(&a), Some(&home), &existing),
        Some(native_exe(&home))
    );
    assert_eq!(find(&[], Some(&home), &existing), Some(native_exe(&home)));
    // home が None なら探さない
    assert_eq!(find(&[a], None, &existing), None);
}

// 要件 §8: home にも無ければ None（home 直下の claude.exe などは拾わない）
#[test]
fn find_claude_none_when_nowhere() {
    let home = PathBuf::from(r"C:\Users\K");
    let existing = fs(&[
        home.join("claude.exe"),
        home.join(".local").join("claude.exe"),
        home.join("bin").join("claude.exe"),
    ]);
    assert_eq!(
        find(&[PathBuf::from(r"C:\x")], Some(&home), &existing),
        None
    );
    assert_eq!(find(&[], None, &HashSet::new()), None);
}

// ---------------------------------------------------------------------------
// 9. auth_status_label
// ---------------------------------------------------------------------------

/// 実物に近い `claude auth status` の出力（メール・組織名を含む）。
fn status(fields: Value) -> Vec<u8> {
    let mut base = json!({
        "loggedIn": true,
        "authMethod": "claude.ai",
        "apiProvider": "firstParty",
        "email": "someone@example.com",
        "orgId": "11111111-2222-3333-4444-555555555555",
        "orgName": "Example Org",
        "subscriptionType": "max"
    });
    for (k, v) in fields.as_object().unwrap() {
        if v.is_null() {
            base.as_object_mut().unwrap().remove(k);
        } else {
            base[k] = v.clone();
        }
    }
    let mut b = serde_json::to_vec_pretty(&base).unwrap();
    b.push(b'\n');
    b
}

fn assert_no_identity(label: &str) {
    for leak in ["someone", "example", "Example", "@", "Org", "11111111"] {
        assert!(!label.contains(leak), "{label:?} に {leak:?} が出ている");
    }
}

// 要件 §9: サブスク（英数字の subscriptionType を括弧で）。メール・組織名は出さない
#[test]
fn auth_status_subscription_with_type() {
    let got = auth_status_label(&status(json!({})));
    assert_eq!(got, "サブスク（max）");
    assert_no_identity(&got);
    assert_eq!(
        auth_status_label(&status(json!({"subscriptionType": "pro"}))),
        "サブスク（pro）"
    );
    assert_eq!(
        auth_status_label(&status(json!({"subscriptionType": "Max20x"}))),
        "サブスク（Max20x）"
    );
}

// 要件 §9: subscriptionType が無い・文字列でない・英数字以外を含む → 「サブスク」
#[test]
fn auth_status_subscription_type_fallbacks() {
    for t in [
        json!(null),
        json!(5),
        json!(true),
        json!("max plan"),
        json!("someone@example.com"),
        json!("Example Org"),
        json!("pro-team"),
        json!("max\n"),
    ] {
        let got = auth_status_label(&status(json!({"subscriptionType": t})));
        assert_eq!(got, "サブスク", "subscriptionType={t}");
        assert_no_identity(&got);
    }
}

// 要件 §9: api_key → 「API キー」
#[test]
fn auth_status_api_key() {
    let got = auth_status_label(&status(json!({"authMethod": "api_key"})));
    assert_eq!(got, "API キー");
    assert_no_identity(&got);
}

// 要件 §9: それ以外の文字列の authMethod → 「その他の接続」
#[test]
fn auth_status_other_method() {
    for m in ["oauth_token", "bedrock", "Claude.ai", "API_KEY", ""] {
        let got = auth_status_label(&status(json!({"authMethod": m})));
        assert_eq!(got, "その他の接続", "authMethod={m:?}");
        assert_no_identity(&got);
    }
}

// 要件 §9: loggedIn が false → 「ログインしていません」（authMethod があっても）
#[test]
fn auth_status_logged_out() {
    assert_eq!(
        auth_status_label(br#"{"loggedIn": false}"#),
        "ログインしていません"
    );
    assert_eq!(
        auth_status_label(&status(json!({"loggedIn": false}))),
        "ログインしていません"
    );
    assert_eq!(
        auth_status_label(&status(json!({"loggedIn": false, "authMethod": "api_key"}))),
        "ログインしていません"
    );
}

// 要件 §9: JSON として読めない・loggedIn が無い → 「確かめられませんでした」
#[test]
fn auth_status_unreadable() {
    assert_eq!(auth_status_label(b""), "確かめられませんでした");
    assert_eq!(
        auth_status_label(b"Not logged in. Run claude auth login\n"),
        "確かめられませんでした"
    );
    assert_eq!(auth_status_label(b"\xff\xfe"), "確かめられませんでした");
    assert_eq!(
        auth_status_label(&status(json!({"loggedIn": null}))),
        "確かめられませんでした"
    );
    assert_eq!(
        auth_status_label(br#"{"authMethod":"api_key"}"#),
        "確かめられませんでした"
    );
    let got = auth_status_label(br#"{"email":"someone@example.com","orgName":"Example Org"}"#);
    assert_eq!(got, "確かめられませんでした");
}

// ---------------------------------------------------------------------------
// 追補 2 B. mask_secrets
// ---------------------------------------------------------------------------

const DOTS: &str = "••••";

// 要件 追補 2 B: sk- で始まる語は sk- から英数字・_・- が続く限りを •••• に。後ろの文字は残す
#[test]
fn mask_secrets_sk_word_range() {
    assert_eq!(mask_secrets("鍵は sk-ant-abc123 です"), "鍵は •••• です");
    assert_eq!(mask_secrets("sk-abc、次へ"), "••••、次へ");
    assert_eq!(mask_secrets("sk-abc。"), "••••。");
    assert_eq!(mask_secrets("sk-abc日本語"), "••••日本語");
    assert_eq!(mask_secrets("(sk-a_b-c) x"), "(••••) x");
    assert_eq!(mask_secrets("sk-ant.api"), "••••.api");
    assert_eq!(mask_secrets("key=sk-1/2"), "key=••••/2");
}

// 要件 追補 2 B: sk- は大文字小文字を無視
#[test]
fn mask_secrets_sk_case_insensitive() {
    assert_eq!(mask_secrets("SK-ABC def"), "•••• def");
    assert_eq!(mask_secrets("Sk-x"), DOTS);
    assert_eq!(mask_secrets("鍵 sK-Ant-9"), "鍵 ••••");
}

// 要件 追補 2 B: sk- の直前が ASCII 英数字なら伏せない（ask-・RISK-・1sk-）
#[test]
fn mask_secrets_sk_inside_word_kept() {
    for s in ["ask-abc", "task-list", "RISK-FREE", "a1sk-b", "mask-off で"] {
        assert_eq!(mask_secrets(s), s, "{s:?}");
    }
    // 直前が日本語（ASCII 英数字でない）なら伏せる
    assert_eq!(mask_secrets("日本語sk-abc"), "日本語••••");
}

// 要件 追補 2 B: 32 字以上の並びは並び全体を 1 つの ••••。31 字は残す
#[test]
fn mask_secrets_long_run_boundary() {
    let t31 = cycle("a_b-", 31);
    let t32 = cycle("a_b-", 32);
    let t80 = cycle("0123456789abcdefXYZ", 80);
    assert_eq!(
        mask_secrets(&format!("id: {t31} end")),
        format!("id: {t31} end")
    );
    assert_eq!(mask_secrets(&format!("id: {t32} end")), "id: •••• end");
    assert_eq!(mask_secrets(&t80), DOTS);
    assert_eq!(mask_secrets(&format!("x{t80}。")), "••••。");
}

// 要件 追補 2 B: 複数の秘密はそれぞれ伏せる
#[test]
fn mask_secrets_multiple() {
    assert_eq!(mask_secrets("a sk-1 b SK-2 c"), "a •••• b •••• c");
    let hex = cycle("0123456789abcdef", 40);
    assert_eq!(
        mask_secrets(&format!("鍵 sk-ant-xyz と {hex} と {hex}")),
        "鍵 •••• と •••• と ••••"
    );
    // sk- の語が 32 字以上の並びでもあるときは 1 つの ••••
    let long_key = format!("sk-ant-{}", cycle("a1B2c3", 40));
    assert_eq!(mask_secrets(&format!("鍵 {long_key} です")), "鍵 •••• です");
}

// 要件 追補 2 B: 改行は並びを切る（改行をまたいで 32 字と数えない）・改行はそのまま
#[test]
fn mask_secrets_does_not_cross_newlines() {
    let a = cycle("abcd", 20);
    let b = cycle("efgh", 20);
    let s = format!("{a}\n{b}");
    assert_eq!(mask_secrets(&s), s);
    assert_eq!(mask_secrets("sk-abc\ndef"), "••••\ndef");
    assert_eq!(mask_secrets("x\r\nsk-1\r\ny"), "x\r\n••••\r\ny");
}

// 要件 追補 2 B: 秘密が無ければ入力と同じ（URL・パス・日本語・改行・長い日本語・空文字）
#[test]
fn mask_secrets_identity_without_secrets() {
    let jp = "今日はとても良い天気なので散歩に行きましょうそれから買い物もしてから帰りましょうね";
    for s in [
        "",
        "こんにちは！\n今日もがんばろう 🐉",
        "see https://example.com/path?q=1 and C:\\Users\\K\\a.txt",
        "`cargo test` を ~/repo で",
        "比率 1:2、A/B テスト 50%",
        jp,
        "  前後の空白\t\r\n",
    ] {
        assert_eq!(mask_secrets(s), s, "{s:?}");
    }
}

// ---------------------------------------------------------------------------
// 追補 2 C. panel_text は最初に伏せる
// ---------------------------------------------------------------------------

// 要件 追補 2 C: チャット欄にも秘密は出さない（URL・パス・改行は残る）
#[test]
fn panel_text_masks_secrets() {
    assert_eq!(
        panel_text("鍵は sk-ant-abc123 です\r\nhttps://example.com を見て"),
        "鍵は •••• です\nhttps://example.com を見て"
    );
    let hex = cycle("0123456789abcdef", 40);
    assert_eq!(panel_text(&format!("  {hex}\n")), DOTS);
}

// 要件 追補 2 C: 伏せてから 4000 字で切る（切ってから伏せると鍵の先頭が残る）
#[test]
fn panel_text_masks_before_truncating() {
    let key = format!("sk-ant-{}", cycle("a1B2c3", 33));
    // 3990 字＋空白＋40 字の鍵 = 4031 字。先に伏せれば 3995 字で切らない
    let s = format!("{} {key}", "あ".repeat(3990));
    assert_eq!(panel_text(&s), format!("{} ••••", "あ".repeat(3990)));
    // 並び（sk- なし）でも同じ
    let hex = cycle("0123456789abcdef", 40);
    let s = format!("{} {hex}", "あ".repeat(3990));
    assert_eq!(panel_text(&s), format!("{} ••••", "あ".repeat(3990)));
}

// ---------------------------------------------------------------------------
// 追補 2 D. parse_output の失敗の理由
// ---------------------------------------------------------------------------

// 要件 追補 2 D: is_error の理由は秘密を伏せ、パスは残す
#[test]
fn parse_output_failure_reason_masks_secret_keeps_path() {
    let stdout = out_bytes(&json!({
        "type": "result",
        "is_error": true,
        "result": "Invalid API key sk-ant-abc123XYZ in C:\\Users\\K\\.env",
        "session_id": SID
    }));
    assert_eq!(
        parse_output(Some(1), &stdout, b""),
        ChatOutcome::Failed {
            reason: st("Invalid API key •••• in C:\\Users\\K\\.env"),
            session_id: Some(st(SID))
        }
    );
}

// 要件 追補 2 D: 標準エラーの理由も秘密を伏せる（URL は残る）
#[test]
fn parse_output_stderr_reason_masks_secret() {
    let hex = cycle("0123456789abcdef", 40);
    let stderr = format!("\n\nerror: token {hex} rejected by https://api.example.com\nnext\n");
    assert_eq!(
        parse_output(Some(1), b"not json", stderr.as_bytes()),
        ChatOutcome::Failed {
            reason: st("error: token •••• rejected by https://api.example.com"),
            session_id: None
        }
    );
}

// 要件 追補 2 D: 失敗の文は 200 字ちょうどならそのまま、201 字は 199 字＋…
#[test]
fn parse_output_failure_reason_200_201_boundary() {
    for (n, want) in [
        (200, cycle("あい。", 200)),
        (201, format!("{}…", cycle("あい。", 199))),
    ] {
        let text = cycle("あい。", n);
        let stdout = out_bytes(&json!({"type": "result", "is_error": true, "result": text}));
        assert_eq!(
            parse_output(Some(1), &stdout, b""),
            ChatOutcome::Failed {
                reason: want.clone(),
                session_id: None
            },
            "is_error {n} 字"
        );
        assert_eq!(
            parse_output(Some(1), b"", text.as_bytes()),
            ChatOutcome::Failed {
                reason: want,
                session_id: None
            },
            "stderr {n} 字"
        );
    }
}

// 要件 追補 2 D: 伏せてから 200 字で切る（境界をまたぐ鍵の先頭を残さない）
#[test]
fn parse_output_failure_reason_masks_before_truncating() {
    let key = format!("sk-ant-{}", cycle("a1B2c3", 33));
    // 195 字＋空白＋40 字の鍵。先に伏せれば 200 字ちょうどで切らない
    let text = format!("{} {key}", cycle("あい。", 195));
    let want = format!("{} ••••", cycle("あい。", 195));
    let stdout = out_bytes(&json!({"type": "result", "is_error": true, "result": text}));
    assert_eq!(
        parse_output(Some(1), &stdout, b""),
        ChatOutcome::Failed {
            reason: want.clone(),
            session_id: None
        }
    );
    assert_eq!(
        parse_output(None, b"", text.as_bytes()),
        ChatOutcome::Failed {
            reason: want,
            session_id: None
        }
    );
}

// 要件 追補 2 D: 失敗の理由は 140 字で切らない・bubble_text の置き換え文にならない
#[test]
fn parse_output_failure_reason_not_bubble_text() {
    let text = format!("{} `code` www.example.com", cycle("あい。", 150));
    let stdout = out_bytes(&json!({"type": "result", "is_error": true, "result": text}));
    match parse_output(Some(1), &stdout, b"") {
        ChatOutcome::Failed { reason, .. } => {
            assert_eq!(reason, text);
            assert_ne!(reason, BUBBLE_HIDDEN);
        }
        other => panic!("Failed のはず: {other:?}"),
    }
}

// 要件 追補 2 D: 成功の答え（Answer.text）は従来どおり生の文字列（伏せない）
#[test]
fn parse_output_answer_text_not_masked() {
    let stdout = out_bytes(&json!({"type": "result", "result": "鍵は sk-ant-abc です"}));
    assert_eq!(
        parse_output(Some(0), &stdout, b""),
        ChatOutcome::Answer {
            text: st("鍵は sk-ant-abc です"),
            session_id: None
        }
    );
}

// ---------------------------------------------------------------------------
// 追補 2 E. input_text
// ---------------------------------------------------------------------------

// 要件 追補 2 E: 前後の空白を落とし、内部の改行は残す
#[test]
fn input_text_trims_edges_keeps_inner() {
    assert_eq!(input_text("  こんにちは \n"), Some(st("こんにちは")));
    assert_eq!(
        input_text("\r\n一行目\n\n二行目\t"),
        Some(st("一行目\n\n二行目"))
    );
    assert_eq!(input_text("a  b"), Some(st("a  b")));
}

// 要件 追補 2 E: 空・空白だけは None
#[test]
fn input_text_empty_is_none() {
    for s in ["", " ", "\n\r\n\t ", "   \n   "] {
        assert_eq!(input_text(s), None, "{s:?}");
    }
}

// 要件 追補 2 E: 2000 字はそのまま、2001 字は切り詰めずに None（字数は chars().count()）
#[test]
fn input_text_2000_2001_boundary() {
    let s2000 = "あ".repeat(2000);
    assert_eq!(input_text(&s2000), Some(s2000.clone()));
    assert_eq!(input_text(&"あ".repeat(2001)), None);
    assert_eq!(input_text(&"x".repeat(2001)), None);
    // 6000 バイトだが 2000 字なので通る
    assert_eq!(s2000.len(), 6000);
}

// 要件 追補 2 E: 前後の空白は数える前に落とす（questions.md Q12）
#[test]
fn input_text_trims_before_counting() {
    let body = "あ".repeat(2000);
    assert_eq!(input_text(&format!("  \n{body}\n  ")), Some(body));
}

// 要件 追補 2 E: 秘密や URL を含んでも本文はそのまま（伏せるのは表示の時）
#[test]
fn input_text_does_not_mask() {
    let s = "この鍵 sk-ant-abc は https://example.com で使う";
    assert_eq!(input_text(s), Some(st(s)));
}
