//! hook（`pet-hook.exe`）とアプリ（`pet-app.exe`）のあいだの通信の型と、最小限の HTTP/1.1。
//!
//! ソケットにもファイルにも触らず、バイト列の読み書きだけを持つ。受け取る側は
//! loopback で待ち受けるが、同じ PC のブラウザのページからも叩けるので、`Origin` 付きの
//! 要求の拒否と token の照合（`is_authorized`）で経路を絞る。入力は全部信頼できない外部データ。
//! 仕様の正本は `.steering/20261021-phase2-hooks/requirements.md`。

use serde::{Deserialize, Serialize};

use crate::hook::HookInput;

/// 1 つの要求として受け付ける最大のバイト数（ヘッダ＋本文）。
///
/// 承認の要約は 120 文字までなので、通常の要求は数 KB。上限が無いと、接続してきた相手が
/// 延々と送ってアプリのメモリを食える。
pub const MAX_REQUEST_BYTES: usize = 131_072;

/// アプリが起動時に書く発見ファイルの中身（JSON）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Discovery {
    /// 例 `"127.0.0.1:53211"`。
    pub endpoint: String,
    pub token: String,
    pub pid: u32,
}

/// hook からアプリへ送る出来事（HTTP の本文の JSON）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireEvent {
    /// `"prompt"` | `"tool_start"` | `"permission"` | `"notification"` | `"stop"` |
    /// `"stop_failure"` | `"other"`
    pub kind: String,
    pub session: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// notification の種類（`"permission_prompt"` など）や stop_failure の error、other のイベント名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// 承認の状態（`GET /permission/<id>` の応答の JSON）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    Pending,
    Allow,
    Deny,
    Ask,
}

/// 読み取った HTTP 要求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// `"GET"` / `"POST"`（大文字のまま）。
    pub method: String,
    /// 例 `"/event"`、`"/permission/abc"`。クエリ込みでそのまま。
    pub path: String,
    /// `Authorization` ヘッダの値（前後の空白を除く）。
    pub authorization: Option<String>,
    /// `Origin` ヘッダの値。
    pub origin: Option<String>,
    pub body: Vec<u8>,
}

/// `parse_request` の失敗。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// まだ全部届いていない（読み続ける）。
    Incomplete,
    /// HTTP として壊れている。
    Malformed,
    /// 上限（`MAX_REQUEST_BYTES`）を超える。
    TooLarge,
}

impl From<&HookInput> for WireEvent {
    fn from(input: &HookInput) -> Self {
        let mk = |kind: &str, session: &str| WireEvent {
            kind: kind.to_string(),
            session: session.to_string(),
            tool: None,
            summary: None,
            detail: None,
        };
        match input {
            HookInput::Prompt { session } => mk("prompt", session),
            HookInput::ToolStart { session, tool } => WireEvent {
                tool: Some(tool.clone()),
                ..mk("tool_start", session)
            },
            HookInput::PermissionRequest {
                session,
                tool,
                summary,
            } => WireEvent {
                tool: Some(tool.clone()),
                summary: Some(summary.clone()),
                ..mk("permission", session)
            },
            HookInput::Notification { session, kind } => WireEvent {
                detail: Some(kind.as_str().to_string()),
                ..mk("notification", session)
            },
            HookInput::Stop { session } => mk("stop", session),
            HookInput::StopFailure { session, error } => WireEvent {
                detail: Some(error.clone()),
                ..mk("stop_failure", session)
            },
            HookInput::Other { session, event } => WireEvent {
                detail: Some(event.clone()),
                ..mk("other", session)
            },
        }
    }
}

const HEADER_END: &[u8] = b"\r\n\r\n";

/// 最初の `\r\n\r\n` の位置。
fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(HEADER_END.len()).position(|w| w == HEADER_END)
}

/// SP・HT だけを前後から除く（HTTP のヘッダ値の空白）。
fn trim_ows(s: &str) -> &str {
    s.trim_matches(|c| c == ' ' || c == '\t')
}

/// ヘッダ名を重複なしで 1 度だけ入れるための入れ物。
fn set_once(slot: &mut Option<String>, value: &str) -> Result<(), ParseError> {
    if slot.is_some() {
        return Err(ParseError::Malformed);
    }
    *slot = Some(value.to_string());
    Ok(())
}

/// アプリ側が受けた HTTP/1.1 の要求を読む。
///
/// 検査の順序は固定: ①全体の上限 ②ヘッダの終わり ③ヘッダの UTF-8 ④要求行 ⑤ヘッダ
/// ⑥宣言された長さの上限 ⑦本文の到着。ヘッダは最後の行まで検査し、1 つでも違反があれば
/// `Malformed`（`Content-Length` が大きすぎる場合の `TooLarge` より先）。
pub fn parse_request(buf: &[u8]) -> Result<Request, ParseError> {
    // ①
    if buf.len() > MAX_REQUEST_BYTES {
        return Err(ParseError::TooLarge);
    }
    // ②
    let head_len = find_header_end(buf).ok_or(ParseError::Incomplete)?;
    let header_end = head_len + HEADER_END.len();
    // ③
    let head = std::str::from_utf8(&buf[..head_len]).map_err(|_| ParseError::Malformed)?;

    let mut lines = head.split("\r\n");
    // ④ 半角空白 1 つずつでちょうど 3 つに分ける。
    let request_line = lines.next().unwrap_or("");
    let parts: Vec<&str> = request_line.split(' ').collect();
    let [method, path, version] = parts[..] else {
        return Err(ParseError::Malformed);
    };
    if method.is_empty()
        || !version.starts_with("HTTP/1.")
        || !path.starts_with('/')
        || has_bare_line_break(request_line)
    {
        return Err(ParseError::Malformed);
    }

    // ⑤
    let mut content_length_seen = false;
    // 数字だが usize に収まらない値は、⑤では TooLarge にせず印だけ付けて⑥まで持ち越す。
    let mut content_length: Option<usize> = None;
    let mut content_length_overflow = false;
    let mut authorization: Option<String> = None;
    let mut origin: Option<String> = None;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(ParseError::Malformed)?;
        if name.is_empty() || name.contains([' ', '\t']) || has_bare_line_break(line) {
            return Err(ParseError::Malformed);
        }
        let value = trim_ows(value);
        if name.eq_ignore_ascii_case("content-length") {
            if content_length_seen {
                return Err(ParseError::Malformed);
            }
            content_length_seen = true;
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ParseError::Malformed);
            }
            match value.parse::<usize>() {
                Ok(n) => content_length = Some(n),
                Err(_) => content_length_overflow = true,
            }
        } else if name.eq_ignore_ascii_case("authorization") {
            set_once(&mut authorization, value)?;
        } else if name.eq_ignore_ascii_case("origin") {
            set_once(&mut origin, value)?;
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(ParseError::Malformed);
        }
    }

    // ⑥ ヘッダの長さ（終わりの 4 バイトを含む）＋宣言された本文の長さで上限を見る。
    if content_length_overflow {
        return Err(ParseError::TooLarge);
    }
    let body_len = content_length.unwrap_or(0);
    let total = header_end
        .checked_add(body_len)
        .ok_or(ParseError::TooLarge)?;
    if total > MAX_REQUEST_BYTES {
        return Err(ParseError::TooLarge);
    }
    // ⑦
    if buf.len() < total {
        return Err(ParseError::Incomplete);
    }

    Ok(Request {
        method: method.to_string(),
        path: path.to_string(),
        authorization,
        origin,
        body: buf[header_end..total].to_vec(),
    })
}

/// 行の中に CRLF の一部でない単独の CR・LF があるか。ヘッダの値に紛れ込ませて
/// 別の行に見せかける手口（要求の密輸）を、要件に無いが念のため断つ。
fn has_bare_line_break(line: &str) -> bool {
    line.contains(['\r', '\n'])
}

/// HTTP 応答のバイト列。
pub fn format_response(status: u16, body: &str) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        413 => "Payload Too Large",
        _ => "Error",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// hook からアプリへ送る HTTP 要求のバイト列。
///
/// 前提は `method`・`path`・`token` が空でなく、空白・CR・LF を含まず、`path` が `/` で
/// 始まること。守られていない入力の振る舞いは定めない。
pub fn format_request(method: &str, path: &str, token: &str, body: &str) -> Vec<u8> {
    format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// 要求を受け付けてよいか。
///
/// `Origin` 付きは常に拒否する（ブラウザのページから loopback を叩かれる経路を断つ）。
/// それ以外は `Authorization` が `Bearer <token>` と完全に一致するときだけ許可する。
pub fn is_authorized(req: &Request, token: &str) -> bool {
    if req.origin.is_some() || token.is_empty() {
        return false;
    }
    let Some(auth) = req.authorization.as_deref() else {
        return false;
    };
    let expected = format!("Bearer {token}");
    constant_time_eq(auth.as_bytes(), expected.as_bytes())
}

/// 長さが同じなら全バイトを比べ終えてから結果を返す比較。途中で抜けないので、
/// 一致した先頭の長さが応答時間から漏れない。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    // ミューテーションで見逃された 2 件を塞ぐ（tests/ は固定なのでここに置く。2026-10-21 司令塔）

    #[test]
    fn constant_time_eq_does_not_let_differences_cancel() {
        // 2 バイトの差がどちらも 0x01。XOR で畳むと打ち消し合って一致に見える
        assert!(!constant_time_eq(b"ab", b"`c"));
        assert!(constant_time_eq(b"ab", b"ab"));
        assert!(!constant_time_eq(b"ab", b"abc"));
    }

    #[test]
    fn bare_line_breaks_are_detected() {
        assert!(has_bare_line_break("X: a\nb"));
        assert!(has_bare_line_break("X: a\rb"));
        assert!(!has_bare_line_break("X: ab"));
    }
}
