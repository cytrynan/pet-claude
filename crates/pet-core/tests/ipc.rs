//! `pet_core::ipc` の受け入れテスト（テスト先行）。
//!
//! 原本: `.steering/20261021-phase2-hooks/requirements.md` の 6〜9 節。公開 API だけを使う。

use pet_core::hook::{HookInput, NotificationKind};
use pet_core::ipc::{
    Discovery, MAX_REQUEST_BYTES, ParseError, PermissionState, Request, WireEvent, format_request,
    format_response, is_authorized, parse_request,
};
use serde_json::{Value, json};

fn st(x: &str) -> String {
    x.to_string()
}

fn pr(s: &str) -> Result<Request, ParseError> {
    parse_request(s.as_bytes())
}

fn request(
    method: &str,
    path: &str,
    authorization: Option<&str>,
    origin: Option<&str>,
    body: &[u8],
) -> Request {
    Request {
        method: st(method),
        path: st(path),
        authorization: authorization.map(st),
        origin: origin.map(st),
        body: body.to_vec(),
    }
}

// ---------------------------------------------------------------------------
// 6. WireEvent の対応
// ---------------------------------------------------------------------------

fn wire(
    kind: &str,
    session: &str,
    tool: Option<&str>,
    summary: Option<&str>,
    detail: Option<&str>,
) -> WireEvent {
    WireEvent {
        kind: st(kind),
        session: st(session),
        tool: tool.map(st),
        summary: summary.map(st),
        detail: detail.map(st),
        origin: None,
        questions: None,
    }
}

// 要件 6: HookInput → WireEvent の対応表
#[test]
fn wire_event_from_hook_input() {
    let cases = [
        (
            HookInput::Prompt { session: st("s") },
            wire("prompt", "s", None, None, None),
        ),
        (
            HookInput::ToolStart {
                session: st("s"),
                tool: st("Bash"),
            },
            wire("tool_start", "s", Some("Bash"), None, None),
        ),
        (
            HookInput::PermissionRequest {
                session: st("s"),
                tool: st("Write"),
                summary: st("C:/a.txt"),
            },
            wire("permission", "s", Some("Write"), Some("C:/a.txt"), None),
        ),
        (
            HookInput::Notification {
                session: st("s"),
                kind: NotificationKind::PermissionPrompt,
            },
            wire("notification", "s", None, None, Some("permission_prompt")),
        ),
        (
            HookInput::Notification {
                session: st("s"),
                kind: NotificationKind::IdlePrompt,
            },
            wire("notification", "s", None, None, Some("idle_prompt")),
        ),
        (
            HookInput::Notification {
                session: st("s"),
                kind: NotificationKind::ElicitationDialog,
            },
            wire("notification", "s", None, None, Some("elicitation_dialog")),
        ),
        (
            HookInput::Notification {
                session: st("s"),
                kind: NotificationKind::Other(st("auth_success")),
            },
            wire("notification", "s", None, None, Some("auth_success")),
        ),
        (
            HookInput::Notification {
                session: st("s"),
                kind: NotificationKind::Other(String::new()),
            },
            wire("notification", "s", None, None, Some("")),
        ),
        (
            HookInput::Stop { session: st("s") },
            wire("stop", "s", None, None, None),
        ),
        (
            HookInput::StopFailure {
                session: st("s"),
                error: st("rate_limit"),
            },
            wire("stop_failure", "s", None, None, Some("rate_limit")),
        ),
        (
            HookInput::Other {
                session: st("s"),
                event: st("SessionStart"),
            },
            wire("other", "s", None, None, Some("SessionStart")),
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(WireEvent::from(&input), expected, "input = {input:?}");
    }
}

// 要件 6: session・tool・summary・detail はそのまま（空文字・改行入りでも）
#[test]
fn wire_event_keeps_strings_verbatim() {
    let got = WireEvent::from(&HookInput::PermissionRequest {
        session: st(""),
        tool: st("Bash\n"),
        summary: String::new(),
    });
    assert_eq!(got, wire("permission", "", Some("Bash\n"), Some(""), None));
    let got = WireEvent::from(&HookInput::StopFailure {
        session: st("a\tb"),
        error: String::new(),
    });
    assert_eq!(got, wire("stop_failure", "a\tb", None, None, Some("")));
}

// 要件 6: JSON にするとき None の項目は出さない（Some("") は出す）
#[test]
fn wire_event_serializes_without_none_fields() {
    let v: Value =
        serde_json::to_value(WireEvent::from(&HookInput::Prompt { session: st("s") })).unwrap();
    assert_eq!(v, json!({ "kind": "prompt", "session": "s" }));

    let v: Value = serde_json::to_value(WireEvent::from(&HookInput::PermissionRequest {
        session: st("s"),
        tool: st("Bash"),
        summary: st("ls"),
    }))
    .unwrap();
    assert_eq!(
        v,
        json!({ "kind": "permission", "session": "s", "tool": "Bash", "summary": "ls" })
    );

    let v: Value = serde_json::to_value(WireEvent::from(&HookInput::Notification {
        session: st("s"),
        kind: NotificationKind::Other(String::new()),
    }))
    .unwrap();
    assert_eq!(
        v,
        json!({ "kind": "notification", "session": "s", "detail": "" })
    );

    let v: Value =
        serde_json::to_value(wire("permission", "s", Some("T"), Some(""), None)).unwrap();
    assert_eq!(
        v,
        json!({ "kind": "permission", "session": "s", "tool": "T", "summary": "" })
    );

    let text = serde_json::to_string(&wire("stop", "s", None, None, None)).unwrap();
    assert!(!text.contains("null"), "None が null として出た: {text}");
}

// 要件 6: 読むときは None の項目が無くてよい・往復できる
#[test]
fn wire_event_deserializes_and_round_trips() {
    let got: WireEvent = serde_json::from_str(r#"{"kind":"stop","session":"s"}"#).unwrap();
    assert_eq!(got, wire("stop", "s", None, None, None));

    let got: WireEvent =
        serde_json::from_str(r#"{"session":"s","detail":"idle_prompt","kind":"notification"}"#)
            .unwrap();
    assert_eq!(
        got,
        wire("notification", "s", None, None, Some("idle_prompt"))
    );

    for w in [
        wire("prompt", "s", None, None, None),
        wire("permission", "s\n", Some("Bash"), Some("ls -la"), None),
        wire("other", "", None, None, Some("")),
    ] {
        let text = serde_json::to_string(&w).unwrap();
        let back: WireEvent = serde_json::from_str(&text).unwrap();
        assert_eq!(back, w);
    }
}

// 承認の来た先（`origin`）: 付けた時だけ JSON に出る。古い pet-hook の JSON（欄が無い）も読める
#[test]
fn wire_event_origin_is_optional() {
    let mut w = wire("permission", "s", Some("Bash"), Some("ls"), None);
    w.origin = Some(st("desktop_app"));
    let v: Value = serde_json::to_value(&w).unwrap();
    assert_eq!(v["origin"], json!("desktop_app"));
    let back: WireEvent = serde_json::from_value(v).unwrap();
    assert_eq!(back, w);

    let old: WireEvent =
        serde_json::from_str(r#"{"kind":"permission","session":"s","tool":"Bash","summary":"ls"}"#)
            .unwrap();
    assert_eq!(old.origin, None);
    // HookInput からは付かない（環境を読む pet-hook が付ける）
    let from_input = WireEvent::from(&HookInput::PermissionRequest {
        session: st("s"),
        tool: st("Bash"),
        summary: st("ls"),
    });
    assert_eq!(from_input.origin, None);
}

// 要件 識別子契約: Discovery と PermissionState の JSON
#[test]
fn discovery_and_permission_state_serde() {
    let d = Discovery {
        endpoint: st("127.0.0.1:53211"),
        token: st("tok"),
        pid: 4242,
    };
    let v = serde_json::to_value(&d).unwrap();
    assert_eq!(
        v,
        json!({ "endpoint": "127.0.0.1:53211", "token": "tok", "pid": 4242 })
    );
    let back: Discovery = serde_json::from_value(v).unwrap();
    assert_eq!(back, d);

    for (state, text) in [
        (PermissionState::Pending, "\"pending\""),
        (PermissionState::Allow, "\"allow\""),
        (PermissionState::Deny, "\"deny\""),
        (PermissionState::Ask, "\"ask\""),
    ] {
        assert_eq!(serde_json::to_string(&state).unwrap(), text);
        assert_eq!(
            serde_json::from_str::<PermissionState>(text).unwrap(),
            state
        );
    }
    assert!(serde_json::from_str::<PermissionState>("\"Allow\"").is_err());
}

// ---------------------------------------------------------------------------
// 7. parse_request
// ---------------------------------------------------------------------------

// 要件 7: 本文の無い GET
#[test]
fn parse_request_simple_get() {
    assert_eq!(
        pr("GET /permission/abc HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"),
        Ok(request("GET", "/permission/abc", None, None, b""))
    );
    // ヘッダが 1 つも無い
    assert_eq!(
        pr("GET / HTTP/1.1\r\n\r\n"),
        Ok(request("GET", "/", None, None, b""))
    );
    // HTTP/1.0 も受ける
    assert_eq!(
        pr("GET /x HTTP/1.0\r\n\r\n"),
        Ok(request("GET", "/x", None, None, b""))
    );
}

// 要件 7: POST の本文・Authorization・Origin・クエリ込みの PATH
#[test]
fn parse_request_post_with_headers() {
    let raw = "POST /event?x=1&y=%20 HTTP/1.1\r\n\
               Host: 127.0.0.1:53211\r\n\
               Authorization: Bearer tok\r\n\
               Origin: http://evil.example\r\n\
               Content-Type: application/json\r\n\
               Content-Length: 2\r\n\
               Connection: close\r\n\
               \r\n\
               {}";
    assert_eq!(
        pr(raw),
        Ok(request(
            "POST",
            "/event?x=1&y=%20",
            Some("Bearer tok"),
            Some("http://evil.example"),
            b"{}"
        ))
    );
}

// 要件 7: ヘッダ名は大文字小文字を区別しない
#[test]
fn parse_request_header_names_case_insensitive() {
    let raw =
        "POST /e HTTP/1.1\r\ncontent-LENGTH: 3\r\nAUTHORIZATION: Bearer t\r\noRiGiN: o\r\n\r\nabc";
    assert_eq!(
        pr(raw),
        Ok(request("POST", "/e", Some("Bearer t"), Some("o"), b"abc"))
    );
}

// 要件 7: 値は前後の SP・HT だけを除く。値が空なら Some("")
#[test]
fn parse_request_header_value_trimming() {
    let raw = "GET / HTTP/1.1\r\nAuthorization: \t Bearer tok \t \r\nOrigin:\t\r\n\r\n";
    assert_eq!(
        pr(raw),
        Ok(request("GET", "/", Some("Bearer tok"), Some(""), b""))
    );

    // コロンの直後に空白が無くてもよい・値の中の空白とコロンは残す
    let raw =
        "GET / HTTP/1.1\r\nAuthorization:Bearer  a b\r\nOrigin: http://127.0.0.1:3000\r\n\r\n";
    assert_eq!(
        pr(raw),
        Ok(request(
            "GET",
            "/",
            Some("Bearer  a b"),
            Some("http://127.0.0.1:3000"),
            b""
        ))
    );

    // 空の値
    let raw = "GET / HTTP/1.1\r\nAuthorization:\r\nOrigin:   \r\n\r\n";
    assert_eq!(pr(raw), Ok(request("GET", "/", Some(""), Some(""), b"")));

    // SP・HT 以外（VT・全角空白）は除かない
    let raw = "GET / HTTP/1.1\r\nAuthorization: Bearer t\u{b}\r\nOrigin: \u{3000}o\r\n\r\n";
    assert_eq!(
        pr(raw),
        Ok(request(
            "GET",
            "/",
            Some("Bearer t\u{b}"),
            Some("\u{3000}o"),
            b""
        ))
    );
}

// 要件 7: 本文は Content-Length バイトだけ（余りは捨てる）・最初の \r\n\r\n がヘッダの終わり
#[test]
fn parse_request_body_handling() {
    assert_eq!(
        pr("POST /e HTTP/1.1\r\nContent-Length: 3\r\n\r\nabcdef"),
        Ok(request("POST", "/e", None, None, b"abc"))
    );
    assert_eq!(
        pr("POST /e HTTP/1.1\r\nContent-Length: 0\r\n\r\nabc"),
        Ok(request("POST", "/e", None, None, b""))
    );
    // Content-Length が無ければ本文は空（後ろに何があっても）
    assert_eq!(
        pr("POST /e HTTP/1.1\r\nHost: x\r\n\r\nleftover"),
        Ok(request("POST", "/e", None, None, b""))
    );
    // 本文の中の \r\n\r\n は本文
    assert_eq!(
        pr("POST /e HTTP/1.1\r\nContent-Length: 8\r\n\r\nab\r\n\r\ncdXX"),
        Ok(request("POST", "/e", None, None, b"ab\r\n\r\ncd"))
    );
    // 本文が \r\n\r\n で始まる
    assert_eq!(
        pr("POST /e HTTP/1.1\r\nContent-Length: 4\r\n\r\n\r\n\r\n"),
        Ok(request("POST", "/e", None, None, b"\r\n\r\n"))
    );
    // 前後に SP・HT がある Content-Length・先頭の 0
    assert_eq!(
        pr("POST /e HTTP/1.1\r\nContent-Length: \t 3 \t\r\n\r\nabc"),
        Ok(request("POST", "/e", None, None, b"abc"))
    );
    assert_eq!(
        pr("POST /e HTTP/1.1\r\nContent-Length: 003\r\n\r\nabc"),
        Ok(request("POST", "/e", None, None, b"abc"))
    );
}

// 要件 7: 本文は UTF-8 でなくてよい（③はヘッダ部だけを見る）
#[test]
fn parse_request_body_may_be_binary() {
    let mut raw = b"POST /e HTTP/1.1\r\nContent-Length: 4\r\n\r\n".to_vec();
    raw.extend_from_slice(&[0xFF, 0xFE, 0x00, 0xC3]);
    assert_eq!(
        parse_request(&raw),
        Ok(request("POST", "/e", None, None, &[0xFF, 0xFE, 0x00, 0xC3]))
    );
}

// 要件 7: ヘッダの終わりがまだ無ければ Incomplete
#[test]
fn parse_request_incomplete_header() {
    for partial in [
        "",
        "G",
        "GET / HTTP/1.1",
        "GET / HTTP/1.1\r\n",
        "GET / HTTP/1.1\r\nHost: x\r\n",
        "GET / HTTP/1.1\r\nHost: x\r\n\r",
        // ヘッダが壊れていても、終わりが無ければ Incomplete（②が③〜⑤より先）
        "GARBAGE\r\nno colon here\r\n",
        "GET / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n",
    ] {
        assert_eq!(
            pr(partial),
            Err(ParseError::Incomplete),
            "buf = {partial:?}"
        );
    }
    // UTF-8 でないヘッダでも、終わりが無ければ Incomplete
    assert_eq!(
        parse_request(b"GET /\xFF HTTP/1.1\r\n"),
        Err(ParseError::Incomplete)
    );
}

// 要件 7: 改行が \n だけの要求は終わりが見つからず、上限で打ち切られる
#[test]
fn parse_request_lf_only_never_completes() {
    let lf = "GET / HTTP/1.1\nHost: x\n\n";
    assert_eq!(pr(lf), Err(ParseError::Incomplete));
    let mut big = lf.as_bytes().to_vec();
    big.resize(MAX_REQUEST_BYTES, b'a');
    assert_eq!(parse_request(&big), Err(ParseError::Incomplete));
    big.push(b'a');
    assert_eq!(parse_request(&big), Err(ParseError::TooLarge));
}

// 要件 7: 本文が足りなければ Incomplete
#[test]
fn parse_request_incomplete_body() {
    assert_eq!(
        pr("POST /e HTTP/1.1\r\nContent-Length: 5\r\n\r\n"),
        Err(ParseError::Incomplete)
    );
    assert_eq!(
        pr("POST /e HTTP/1.1\r\nContent-Length: 5\r\n\r\nabcd"),
        Err(ParseError::Incomplete)
    );
    assert_eq!(
        pr("POST /e HTTP/1.1\r\nContent-Length: 5\r\n\r\nabcde"),
        Ok(request("POST", "/e", None, None, b"abcde"))
    );
}

// 要件 7: ③ヘッダが UTF-8 でなければ Malformed（使わないヘッダの値でも）
#[test]
fn parse_request_non_utf8_header_is_malformed() {
    assert_eq!(
        parse_request(b"GET /\xFF HTTP/1.1\r\n\r\n"),
        Err(ParseError::Malformed)
    );
    assert_eq!(
        parse_request(b"GET / HTTP/1.1\r\nX-Foo: \xC3\x28\r\n\r\n"),
        Err(ParseError::Malformed)
    );
}

// 要件 7: 要求行は半角空白 1 つずつでちょうど 3 つ・HTTP/1. で始まる・PATH は / で始まる
#[test]
fn parse_request_bad_request_line() {
    for line in [
        "GET  / HTTP/1.1",
        "GET /  HTTP/1.1",
        "GET / HTTP/1.1 ",
        " GET / HTTP/1.1",
        "GET / HTTP/1.1 extra",
        "GET /",
        "GET",
        "",
        "GET\t/\tHTTP/1.1",
        "GET / HTTP/2",
        "GET / HTTP/2.0",
        "GET / http/1.1",
        "GET / HTTP1.1",
        "GET / FOO",
        "GET x HTTP/1.1",
        "GET * HTTP/1.1",
        "GET http://127.0.0.1/ HTTP/1.1",
    ] {
        let raw = format!("{line}\r\nHost: x\r\n\r\n");
        assert_eq!(pr(&raw), Err(ParseError::Malformed), "line = {line:?}");
    }
}

// 要件 7: 壊れたヘッダ行（: が無い・名前が空・名前の前後に空白）
#[test]
fn parse_request_bad_header_lines() {
    for header in [
        "NoColonHere",
        ":value",
        ": value",
        "Content-Length : 5",
        " Content-Length: 5",
        "X-Foo : bar",
        " X-Foo: bar",
        "Authorization : Bearer t",
        "Origin : o",
    ] {
        let raw = format!("GET / HTTP/1.1\r\nHost: x\r\n{header}\r\n\r\n");
        assert_eq!(pr(&raw), Err(ParseError::Malformed), "header = {header:?}");
    }
}

// 要件 7: 全部のヘッダ行を最後まで検査する（違反が後ろの行にあっても Malformed）
#[test]
fn parse_request_checks_all_header_lines() {
    // 正しい行のあとに壊れた行
    assert_eq!(
        pr("POST /e HTTP/1.1\r\nAuthorization: Bearer t\r\nContent-Length: 2\r\nbroken\r\n\r\n{}"),
        Err(ParseError::Malformed)
    );
    // 本文が足りなくても、ヘッダの違反が先（⑤が⑦より先）
    assert_eq!(
        pr("POST /e HTTP/1.1\r\nContent-Length: 10\r\nbroken\r\n\r\n"),
        Err(ParseError::Malformed)
    );
}

// 要件 7: Content-Length の値は ASCII の数字だけ（符号・空文字・その他は Malformed）
#[test]
fn parse_request_bad_content_length() {
    for value in [
        "", " ", "+5", "-1", "5a", "a5", "0x5", "1 2", "1.0", "５", "1_000", "\u{b}5",
    ] {
        let raw = format!("POST /e HTTP/1.1\r\nContent-Length: {value}\r\n\r\nabcde");
        assert_eq!(
            pr(&raw),
            Err(ParseError::Malformed),
            "Content-Length = {value:?}"
        );
    }
}

// 要件 7: Content-Length・Authorization・Origin が 2 回以上あれば（値が同じでも）Malformed
#[test]
fn parse_request_duplicate_headers() {
    for headers in [
        "Content-Length: 2\r\nContent-Length: 2",
        "Content-Length: 2\r\ncontent-length: 3",
        "Authorization: Bearer t\r\nAuthorization: Bearer t",
        "Authorization: Bearer t\r\nauthorization: Bearer u",
        "Origin: a\r\nOrigin: a",
        "Origin: a\r\nORIGIN: b",
        "Origin:\r\nOrigin:",
    ] {
        let raw = format!("POST /e HTTP/1.1\r\n{headers}\r\n\r\n{{}}");
        assert_eq!(
            pr(&raw),
            Err(ParseError::Malformed),
            "headers = {headers:?}"
        );
    }
    // その他のヘッダは重複しても読み捨てる
    assert_eq!(
        pr(
            "GET / HTTP/1.1\r\nHost: a\r\nHost: b\r\nConnection: close\r\nConnection: keep-alive\r\nX-Foo:\r\n\r\n"
        ),
        Ok(request("GET", "/", None, None, b""))
    );
}

// 要件 7: Transfer-Encoding があれば（本文が届いていなくても）Malformed
#[test]
fn parse_request_transfer_encoding_is_malformed() {
    for raw in [
        "POST /e HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
        "POST /e HTTP/1.1\r\ntransfer-encoding: identity\r\n\r\n",
        "POST /e HTTP/1.1\r\nTRANSFER-ENCODING:\r\n\r\n",
        "POST /e HTTP/1.1\r\nContent-Length: 2\r\nTransfer-Encoding: chunked\r\n\r\n{}",
        // 本文がまだ届いていない
        "POST /e HTTP/1.1\r\nContent-Length: 50\r\nTransfer-Encoding: chunked\r\n\r\n",
    ] {
        assert_eq!(pr(raw), Err(ParseError::Malformed), "raw = {raw:?}");
    }
}

/// `POST /event` に `Content-Length: n` を付けたヘッダ部（終わりの \r\n\r\n を含む）。
fn post_header(n: usize) -> String {
    format!("POST /event HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {n}\r\n\r\n")
}

/// ヘッダの長さ＋n がちょうど `MAX_REQUEST_BYTES` になる n。
fn n_for_exact_max() -> usize {
    (0..=MAX_REQUEST_BYTES)
        .rev()
        .find(|&n| post_header(n).len() + n == MAX_REQUEST_BYTES)
        .expect("ちょうどの長さが作れるはず")
}

// 要件 7: ①buf.len() > MAX_REQUEST_BYTES なら TooLarge（ちょうど等しいなら読む）
#[test]
fn parse_request_buffer_size_boundary() {
    let n = n_for_exact_max();
    let mut exact = post_header(n).into_bytes();
    exact.resize(MAX_REQUEST_BYTES, b'b');
    assert_eq!(exact.len(), MAX_REQUEST_BYTES);
    let got = parse_request(&exact).expect("ちょうど上限は読める");
    assert_eq!(got.body, vec![b'b'; n]);

    // 本文の後ろに 1 バイト余分（Content-Length 自体は収まる）→ ①で TooLarge
    let mut over = exact.clone();
    over.push(b'b');
    assert_eq!(parse_request(&over), Err(ParseError::TooLarge));

    // ①は②〜⑤より先: 終わりが無い・UTF-8 でない・壊れていても TooLarge
    assert_eq!(
        parse_request(&vec![b'x'; MAX_REQUEST_BYTES + 1]),
        Err(ParseError::TooLarge)
    );
    let mut bad = b"GET /\xFF HTTP/1.1\r\nbroken\r\n\r\n".to_vec();
    bad.resize(MAX_REQUEST_BYTES + 1, b'z');
    assert_eq!(parse_request(&bad), Err(ParseError::TooLarge));
}

// 要件 7: ⑥ヘッダの長さ＋Content-Length が上限を超えるなら本文を待たずに TooLarge（等しいなら超えない）
#[test]
fn parse_request_declared_length_boundary() {
    let n = n_for_exact_max();
    // ちょうど等しい・本文が届いていない → Incomplete
    assert_eq!(
        parse_request(post_header(n).as_bytes()),
        Err(ParseError::Incomplete)
    );
    // ちょうど等しい・本文が 1 バイト足りない → Incomplete
    let mut short = post_header(n).into_bytes();
    short.resize(MAX_REQUEST_BYTES - 1, b'b');
    assert_eq!(parse_request(&short), Err(ParseError::Incomplete));
    // 1 超える（桁数が同じなのでヘッダの長さは同じ）→ 本文が無くても TooLarge
    assert_eq!(post_header(n + 1).len(), post_header(n).len());
    assert_eq!(
        parse_request(post_header(n + 1).as_bytes()),
        Err(ParseError::TooLarge)
    );
    // 本文が一部届いていても TooLarge
    let mut partial = post_header(n + 1).into_bytes();
    partial.extend_from_slice(b"abc");
    assert_eq!(parse_request(&partial), Err(ParseError::TooLarge));
    // 大きな値
    assert_eq!(
        parse_request(post_header(MAX_REQUEST_BYTES).as_bytes()),
        Err(ParseError::TooLarge)
    );
}

// 要件 7: ⑥ Content-Length が usize に収まらない・usize::MAX でも TooLarge（溢れない）
#[test]
fn parse_request_huge_content_length() {
    let max = usize::MAX.to_string();
    let overflow = format!("{max}0");
    let more = "9".repeat(100);
    for value in [max.as_str(), overflow.as_str(), more.as_str()] {
        let raw = format!("POST /e HTTP/1.1\r\nContent-Length: {value}\r\n\r\n");
        assert_eq!(
            pr(&raw),
            Err(ParseError::TooLarge),
            "Content-Length = {value}"
        );
    }
}

// 要件 7: 収まらない Content-Length でも、ほかのヘッダの違反が先（⑤が⑥より先）
#[test]
fn parse_request_header_violation_before_huge_length() {
    let overflow = format!("{}0", usize::MAX);
    for extra in [
        "broken",
        ":x",
        "Bad : y",
        "Transfer-Encoding: chunked",
        "Authorization: a\r\nAuthorization: a",
    ] {
        // 壊れた行が後ろ
        let raw = format!("POST /e HTTP/1.1\r\nContent-Length: {overflow}\r\n{extra}\r\n\r\n");
        assert_eq!(pr(&raw), Err(ParseError::Malformed), "after: {extra:?}");
        // 壊れた行が前
        let raw = format!("POST /e HTTP/1.1\r\n{extra}\r\nContent-Length: {overflow}\r\n\r\n");
        assert_eq!(pr(&raw), Err(ParseError::Malformed), "before: {extra:?}");
    }
    // 収まらない値の Content-Length が 2 回 → 重複で Malformed
    let raw = format!(
        "POST /e HTTP/1.1\r\nContent-Length: {overflow}\r\nContent-Length: {overflow}\r\n\r\n"
    );
    assert_eq!(pr(&raw), Err(ParseError::Malformed));
    // 上限を超える（usize には収まる）Content-Length と壊れた行 → Malformed
    let raw = format!("POST /e HTTP/1.1\r\nContent-Length: {MAX_REQUEST_BYTES}\r\nbroken\r\n\r\n");
    assert_eq!(pr(&raw), Err(ParseError::Malformed));
    // 符号付きの巨大値は数字だけでないので Malformed
    let raw = format!("POST /e HTTP/1.1\r\nContent-Length: +{overflow}\r\n\r\n");
    assert_eq!(pr(&raw), Err(ParseError::Malformed));
}

// ---------------------------------------------------------------------------
// 8. format_response と format_request
// ---------------------------------------------------------------------------

// 要件 8: format_response の形
#[test]
fn format_response_exact_bytes() {
    assert_eq!(
        format_response(200, "{\"ok\":true}"),
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}".to_vec()
    );
    assert_eq!(
        format_response(404, ""),
        b"HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
    );
}

// 要件 8: reason の対応（それ以外は Error）
#[test]
fn format_response_reasons() {
    for (status, reason) in [
        (200, "OK"),
        (400, "Bad Request"),
        (401, "Unauthorized"),
        (403, "Forbidden"),
        (404, "Not Found"),
        (413, "Payload Too Large"),
        (201, "Error"),
        (204, "Error"),
        (418, "Error"),
        (500, "Error"),
        (503, "Error"),
    ] {
        let expected = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
        );
        assert_eq!(
            format_response(status, "{}"),
            expected.into_bytes(),
            "status = {status}"
        );
    }
}

// 要件 8: Content-Length はバイト数（文字数ではない）
#[test]
fn format_response_content_length_is_bytes() {
    let body = "{\"m\":\"あ😀\"}";
    assert_eq!(body.len(), 15);
    let expected = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{body}"
    );
    assert_eq!(format_response(200, body), expected.into_bytes());
}

// 要件 8: format_request の形
#[test]
fn format_request_exact_bytes() {
    assert_eq!(
        format_request("POST", "/event", "tok", "{}"),
        b"POST /event HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer tok\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec()
    );
    assert_eq!(
        format_request("GET", "/permission/abc", "t0k-EN_1", ""),
        b"GET /permission/abc HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer t0k-EN_1\r\nContent-Type: application/json\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
    );
    let body = "{\"s\":\"日本\"}";
    let expected = format!(
        "POST /e HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer t\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    assert_eq!(
        format_request("POST", "/e", "t", body),
        expected.into_bytes()
    );
}

// 要件 8: format_request の出力は parse_request でそのまま読める（往復）
#[test]
fn format_request_round_trips_through_parse_request() {
    let big = "x".repeat(100_000);
    let cases = [
        ("POST", "/event", "tok", "{}"),
        ("GET", "/permission/abc", "0123456789abcdef", ""),
        ("POST", "/event?x=1&y=2", "t", "{\"a\":\"\\r\\n\"}"),
        ("POST", "/e", "t", "line1\r\n\r\nline2"),
        ("POST", "/e", "t", "日本語😀"),
        ("POST", "/", "tok", big.as_str()),
        ("PUT", "/a/b/c", "TOKEN", "body"),
    ];
    for (method, path, token, body) in cases {
        let bytes = format_request(method, path, token, body);
        let got = parse_request(&bytes);
        let auth = format!("Bearer {token}");
        assert_eq!(
            got,
            Ok(request(method, path, Some(&auth), None, body.as_bytes())),
            "method = {method}, path = {path}"
        );
        assert!(is_authorized(&got.unwrap(), token), "path = {path}");
    }
}

// ---------------------------------------------------------------------------
// 9. is_authorized
// ---------------------------------------------------------------------------

// 要件 9: Bearer <token> と完全一致するときだけ true
#[test]
fn is_authorized_exact_match() {
    let ok = request("POST", "/event", Some("Bearer tok"), None, b"");
    assert!(is_authorized(&ok, "tok"));
    assert!(!is_authorized(&ok, "tok2"));
    assert!(!is_authorized(&ok, "to"));
    assert!(!is_authorized(&ok, "TOK"));
    assert!(!is_authorized(&ok, "abc"));
}

// 要件 9: Bearer の大小・余分な空白・前方一致などは false
#[test]
fn is_authorized_rejects_near_misses() {
    for auth in [
        "bearer tok",
        "BEARER tok",
        "Bearer  tok",
        "Bearer tok ",
        " Bearer tok",
        "Bearer\ttok",
        "Bearertok",
        "Bearer tokX",
        "Bearer to",
        "Bearer TOK",
        "Basic tok",
        "tok",
        "Bearer ",
        "Bearer",
        "",
        "Bearer tok\u{0}",
    ] {
        let req = request("POST", "/event", Some(auth), None, b"");
        assert!(!is_authorized(&req, "tok"), "authorization = {auth:?}");
    }
}

// 要件 9: authorization が None なら false
#[test]
fn is_authorized_none_is_false() {
    let req = request("GET", "/", None, None, b"");
    assert!(!is_authorized(&req, "tok"));
    assert!(!is_authorized(&req, ""));
}

// 要件 9: Origin が付いた要求は常に false（Some("") でも）
#[test]
fn is_authorized_rejects_origin() {
    for origin in ["http://evil.example", "null", "http://127.0.0.1:53211", ""] {
        let req = request("POST", "/event", Some("Bearer tok"), Some(origin), b"");
        assert!(!is_authorized(&req, "tok"), "origin = {origin:?}");
    }
}

// 要件 9: token が空文字のときは常に false
#[test]
fn is_authorized_empty_token_is_false() {
    for auth in ["Bearer ", "Bearer", ""] {
        let req = request("POST", "/event", Some(auth), None, b"");
        assert!(!is_authorized(&req, ""), "authorization = {auth:?}");
    }
}

// 要件 7+9: parse_request が値の前後の空白を除いた結果で判定する・Origin 付きは parse 経由でも false
#[test]
fn is_authorized_via_parse_request() {
    let req = pr("POST /event HTTP/1.1\r\nAuthorization: \t Bearer tok  \r\n\r\n").unwrap();
    assert!(is_authorized(&req, "tok"));

    let req = pr("POST /event HTTP/1.1\r\nAuthorization: Bearer tok\r\nOrigin:\r\n\r\n").unwrap();
    assert!(!is_authorized(&req, "tok"));

    let req = pr("POST /event HTTP/1.1\r\nauthorization: bearer tok\r\n\r\n").unwrap();
    assert!(!is_authorized(&req, "tok"));
}
