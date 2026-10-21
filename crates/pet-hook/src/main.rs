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
    let path = data_dir().join("runtime").join("ipc.json");
    let written = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
    let text = std::fs::read_to_string(&path).ok()?;
    let disc: Discovery = serde_json::from_str(&text).ok()?;
    // アプリが強制終了すると発見ファイルが残る。そのポートで別のプロセスが待ち受けると、承認の答えを
    // 横取りされる（2026-10-21 検証係の指摘）。書いた pid が生きていて、実行ファイルがアプリのときだけ使う
    app_is_alive(disc.pid, written).then_some(disc)
}

/// 発見ファイルを書いたのが、いま生きているアプリか。次の 2 つを両方満たすときだけ true:
///
/// - `pid` の実行ファイルのフルパスが、この hook と同じフォルダの `pet-app.exe`（登録コマンドが同じ
///   フォルダの pet-hook.exe を登録する）。名前だけだと、別の場所に置いた同名の実行ファイルで通る
/// - そのプロセスが発見ファイルより前に生まれている（アプリは起動してから発見ファイルを書く）。
///   pid は再利用されるので、強制終了後に同じ pid を引き当てたプロセスをここで弾く
///
/// （2026-10-21 検証係の指摘 2 巡）。故障注入のテストは `PET_CLAUDE_APP_PATH` で期待するパスを
/// 自分に差し替える。差し替えは `PET_CLAUDE_DATA_DIR`（試験用の置き場所）と一緒の時だけ効く。
fn app_is_alive(pid: u32, written: std::time::SystemTime) -> bool {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    let override_path =
        std::env::var_os("PET_CLAUDE_DATA_DIR").and(std::env::var("PET_CLAUDE_APP_PATH").ok());
    let expected = match override_path {
        Some(p) => p,
        None => match std::env::current_exe() {
            Ok(exe) => exe
                .with_file_name("pet-app.exe")
                .to_string_lossy()
                .into_owned(),
            Err(_) => return false,
        },
    };
    // SAFETY: 読み取りだけの権限で開き、必ず閉じる。出力先はローカル変数
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let named = QueryFullProcessImageNameW(
            h,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let timed = GetProcessTimes(h, &mut created, &mut exited, &mut kernel, &mut user).is_ok();
        let _ = CloseHandle(h);
        if !named || !timed {
            return false;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.eq_ignore_ascii_case(&expected) && filetime_to_system(created) <= written
    }
}

/// FILETIME（1601-01-01 からの 100ns）を SystemTime に。
fn filetime_to_system(ft: windows::Win32::Foundation::FILETIME) -> std::time::SystemTime {
    const UNIX_EPOCH_IN_FILETIME: u64 = 116_444_736_000_000_000;
    let t = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
    let since_unix = t.saturating_sub(UNIX_EPOCH_IN_FILETIME);
    std::time::UNIX_EPOCH + Duration::from_nanos(since_unix.saturating_mul(100))
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
        .unwrap_or(DEFAULT_WAIT_SECS)
        // 巨大な値で Instant の足し算が溢れないように（hook の timeout は 60 秒なので、それ以上は意味が無い）
        .min(DEFAULT_WAIT_SECS * 2);
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
