//! pet-hook: Claude Code の hook から起動され、出来事をペットのアプリへ送る。
//!
//! - 標準入力の JSON を `pet_core::hook::parse` で読み、`WireEvent` にしてアプリへ HTTP で送る
//! - 承認待ち（`PermissionRequest`）だけは、アプリの答えを待って許可・拒否を標準出力に書く。
//!   答えが無い・アプリが居ない・何か失敗したときは `{}`（Claude Code のいつもの流れに戻す）
//! - **どんな失敗でも終了コード 0**。Claude Code を止めない（パニックも捕まえる）
//!
//! 環境変数:
//! - `PET_CLAUDE_DATA_DIR`: 発見ファイルを探す場所（既定 `%LOCALAPPDATA%\pet-claude`）。試験用
//! - `PET_PERMISSION_WAIT_SECS`: 承認の答えを待つ最大秒数（既定 55）

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use pet_core::hook::{self, Decision, HookInput, MAX_INPUT_BYTES};
use pet_core::ipc::{self, Discovery, WireEvent};

const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);
const IO_TIMEOUT: Duration = Duration::from_millis(500);
const POLL_INTERVAL: Duration = Duration::from_millis(300);
const DEFAULT_WAIT_SECS: u64 = 55;
const DENY_MESSAGE: &str = "ペット（pet-claude）の吹き出しで拒否されました";
/// 1 往復の応答を読み切る期限と大きさの上限（アプリの応答は数十バイト）。
const RESPONSE_DEADLINE: Duration = Duration::from_millis(1000);
const MAX_RESPONSE_BYTES: usize = 16 * 1024;
/// 承認の `detail` に付ける印。アプリ側の `ui::can_allow` と同じ値。
const EXACT: &str = "exact";

fn main() {
    let out = std::panic::catch_unwind(run).ok().flatten();
    if let Some(s) = out {
        let mut o = std::io::stdout();
        let _ = o.write_all(s.as_bytes());
        let _ = o.flush();
    }
    std::process::exit(0);
}

fn run() -> Option<String> {
    let mut buf = Vec::new();
    // 上限より 1 バイト多く読めば「超えた」が分かる
    let _ = std::io::stdin()
        .take(MAX_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut buf);
    let input = hook::parse(&buf).ok()?;
    let is_permission = matches!(input, HookInput::PermissionRequest { .. });
    let ask = || is_permission.then(|| hook::permission_output(&Decision::Ask));

    let Some(disc) = discovery() else {
        return ask();
    };
    let mut event = WireEvent::from(&input);
    if is_permission && summary_is_exact(&buf) {
        // 要約が元のコマンドとまったく同じときだけ印を付ける。アプリは印が無ければ吹き出しに
        // 「許可」を出さない（改行を潰した要約・切り詰めた要約を一押しで通させない）
        event.detail = Some(EXACT.to_string());
    }
    let Ok(body) = serde_json::to_string(&event) else {
        return ask();
    };
    if !is_permission {
        let _ = call(&disc, "POST", "/event", &body);
        return None;
    }
    let decision = wait_for_decision(&disc, &body).unwrap_or(Decision::Ask);
    Some(hook::permission_output(&decision))
}

/// 承認の要約が元の文字列と同じか。`parse` は済んでいるので、ここでは要約の元だけ読み直す。
fn summary_is_exact(raw: &[u8]) -> bool {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(raw) else {
        return false;
    };
    let tool = v["tool_name"].as_str().unwrap_or("");
    hook::summary_is_exact(tool, &v["tool_input"])
}

fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("PET_CLAUDE_DATA_DIR") {
        return PathBuf::from(d);
    }
    std::env::var_os("LOCALAPPDATA")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("pet-claude")
}

fn discovery() -> Option<Discovery> {
    let text = std::fs::read_to_string(data_dir().join("runtime").join("ipc.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// 1 往復の HTTP。(状態コード, 本文) を返す。接続できない・時間切れ・壊れた応答は None。
fn call(disc: &Discovery, method: &str, path: &str, body: &str) -> Option<(u16, String)> {
    let addr: SocketAddr = disc.endpoint.parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).ok()?;
    stream.set_read_timeout(Some(IO_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(IO_TIMEOUT)).ok()?;
    stream
        .write_all(&ipc::format_request(method, path, &disc.token, body))
        .ok()?;
    // 読み取りごとの時間切れだけだと、少しずつ返し続ける相手で終われない。応答全体にも期限と上限を置く
    let deadline = Instant::now() + RESPONSE_DEADLINE;
    let mut resp = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if Instant::now() >= deadline || resp.len() > MAX_RESPONSE_BYTES {
            return None;
        }
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => resp.extend_from_slice(&chunk[..n]),
            Err(_) => return None,
        }
    }
    let text = String::from_utf8(resp).ok()?;
    let (head, body) = text.split_once("\r\n\r\n")?;
    let status = head.split(' ').nth(1)?.parse().ok()?;
    Some((status, body.to_string()))
}

fn wait_for_decision(disc: &Discovery, body: &str) -> Option<Decision> {
    let (status, resp) = call(disc, "POST", "/permission", body)?;
    if status != 200 {
        return None;
    }
    let id = serde_json::from_str::<serde_json::Value>(&resp).ok()?["id"]
        .as_str()?
        .to_string();
    let wait = std::env::var("PET_PERMISSION_WAIT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_WAIT_SECS);
    let deadline = Instant::now() + Duration::from_secs(wait);
    while Instant::now() < deadline {
        std::thread::sleep(POLL_INTERVAL);
        let Some((200, resp)) = call(disc, "GET", &format!("/permission/{id}"), "") else {
            // アプリが落ちた・id が消えた
            return None;
        };
        let state = serde_json::from_str::<serde_json::Value>(&resp).ok()?;
        match state["state"].as_str()? {
            "allow" => return Some(Decision::Allow),
            "deny" => {
                return Some(Decision::Deny {
                    message: DENY_MESSAGE.to_string(),
                });
            }
            "ask" => return Some(Decision::Ask),
            _ => {}
        }
    }
    None
}
