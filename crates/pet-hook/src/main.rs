//! pet-hook: Claude Code の hook から起動され、出来事をペットのアプリへ送る。
//!
//! - 標準入力の JSON を `pet_core::hook::parse` で読み、`WireEvent` にしてアプリへ HTTP で送る
//! - 承認待ち（`PermissionRequest`）だけは、アプリの答えを待って許可・拒否を標準出力に書く。
//!   答えが無い・アプリが居ない・何か失敗したときは `{}`（Claude Code のいつもの流れに戻す）
//! - 質問（`PreToolUse` の `AskUserQuestion`）は、`--ask` 引数で起こされたときだけアプリへ送り、承認と同じ
//!   経路で答え（選んだラベル・自由文・拒否）を待って `updatedInput` 付きの JSON を書く。`--ask` の無い
//!   登録（`*` の非同期）から来た分は何もしない（同じ質問を二重に送らない）。答えが無いときは `{}`
//! - **どんな失敗でも終了コード 0**。Claude Code を止めない（パニックも捕まえる）
//!
//! 環境変数:
//! - `PET_CLAUDE_DATA_DIR`: 発見ファイルを探す場所（既定 `%LOCALAPPDATA%\pet-claude`）。試験用
//! - `PET_PERMISSION_WAIT_SECS`: 承認の答えを待つ最大秒数（既定 55）
//! - `PET_ASK_WAIT_SECS`: 質問の答えを待つ最大秒数（既定 290。登録する hook の timeout は 300）

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use pet_core::ask::{AskAnswer, AskRequest, ask_output};
use pet_core::hook::{self, Decision, HookInput, MAX_INPUT_BYTES};
use pet_core::ipc::{self, Discovery, WireEvent};
use pet_core::origin;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);
const IO_TIMEOUT: Duration = Duration::from_millis(500);
const POLL_INTERVAL: Duration = Duration::from_millis(300);
const DEFAULT_WAIT_SECS: u64 = 55;
/// 質問の答えを待つ既定の秒数。人が読んで考える時間を取る（登録する hook の timeout 300 秒より短く）。
const DEFAULT_ASK_WAIT_SECS: u64 = 290;
/// 質問の待ちの上限（hook の timeout 300 秒を超えても意味が無い）。
const MAX_ASK_WAIT_SECS: u64 = 295;
const DENY_MESSAGE: &str = "ペット（pet-claude）の吹き出しで拒否されました";
/// 1 往復の応答を読み切る期限と大きさの上限（アプリの応答は数十バイト。質問の答えでも数 KB）。
const RESPONSE_DEADLINE: Duration = Duration::from_millis(1000);
const MAX_RESPONSE_BYTES: usize = 16 * 1024;
/// 承認の `detail` に付ける印。アプリ側の `ui::can_allow` と同じ値。
const EXACT: &str = "exact";
/// 質問を扱う登録に付ける引数（`install.rs` が登録する）。
const ASK_ARG: &str = "--ask";

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
    if let HookInput::AskQuestion { request, .. } = &input {
        // `*` の非同期登録から来た分は何もしない（同期の `--ask` 登録が送って答えを待つ）
        if !std::env::args().any(|a| a == ASK_ARG) {
            return None;
        }
        return Some(run_ask(request, &input));
    }
    let is_permission = matches!(input, HookInput::PermissionRequest { .. });
    let ask = || is_permission.then(|| hook::permission_output(&Decision::Ask));
    if let HookInput::PermissionRequest { tool, .. } = &input
        && tool == "AskUserQuestion"
    {
        // 質問の承認（auto mode で、質問の hook が「決めない」を返した後に来る。2026-10-27 実測）は
        // Claude Code の画面の質問そのもの。カードにしても要約が無く答えられないので、アプリには送らず任せる
        return ask();
    }

    let Some(disc) = discovery() else {
        return ask();
    };
    let mut event = WireEvent::from(&input);
    if is_permission && summary_is_exact(&buf) {
        // 要約が元のコマンドとまったく同じときだけ印を付ける。アプリは印が無ければ吹き出しに
        // 「許可」を出さない（改行を潰した要約・切り詰めた要約を一押しで通させない）
        event.detail = Some(EXACT.to_string());
    }
    if is_permission {
        // カードの「ターミナルで」をアプリから来た要求では「Claude アプリで」にするため、来た先を添える
        event.origin = Some(permission_origin().as_str().to_string());
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

/// 質問をアプリへ送り、答えを待って hook の出力にする。アプリが居ない・答えが無い・何か失敗なら `{}`
/// （Claude Code のいつもの画面が出る）。
fn run_ask(request: &AskRequest, input: &HookInput) -> String {
    let answer = (|| {
        let disc = discovery()?;
        let mut event = WireEvent::from(input);
        event.origin = Some(permission_origin().as_str().to_string());
        let body = serde_json::to_string(&event).ok()?;
        wait_for_answer(&disc, &body)
    })()
    .unwrap_or(AskAnswer::Ask);
    ask_output(request, &answer)
}

/// 承認の要約が元の文字列と同じか。`parse` は済んでいるので、ここでは要約の元だけ読み直す。
fn summary_is_exact(raw: &[u8]) -> bool {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(raw) else {
        return false;
    };
    let tool = v["tool_name"].as_str().unwrap_or("");
    hook::summary_is_exact(tool, &v["tool_input"])
}

/// 承認の要求の来た先。判定は `pet_core::origin::classify`。ここでは材料（環境変数と、hook の親の
/// Claude Code の実行ファイル）を集めるだけ。`CLAUDE_PID` は公式ドキュメントにある「hook を起こした
/// Claude Code 自身の pid」。
fn permission_origin() -> origin::Origin {
    let entrypoint = std::env::var("CLAUDE_CODE_ENTRYPOINT").ok();
    let exec_path = std::env::var("CLAUDE_CODE_EXECPATH").ok();
    let image = std::env::var("CLAUDE_PID")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .and_then(process_image);
    origin::classify(
        entrypoint.as_deref(),
        exec_path.as_deref(),
        image.as_deref(),
    )
}

/// pid のプロセスの実行ファイルのフルパス。開けない・取れないなら None。
fn process_image(pid: u32) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: 読み取りだけの権限で開き、必ず閉じる
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let path = image_of(h);
        let _ = CloseHandle(h);
        path
    }
}

/// 開いたプロセスの実行ファイルのフルパス。
///
/// # Safety
/// `h` は `PROCESS_QUERY_LIMITED_INFORMATION` 以上で開いた、まだ閉じていないハンドル。
unsafe fn image_of(h: windows::Win32::Foundation::HANDLE) -> Option<String> {
    use windows::Win32::System::Threading::{PROCESS_NAME_WIN32, QueryFullProcessImageNameW};
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: 出力先はローカルの配列で、長さを渡している
    unsafe {
        QueryFullProcessImageNameW(
            h,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    }
    .ok()?;
    Some(String::from_utf16_lossy(&buf[..len as usize]))
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
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
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
        let path = image_of(h);
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let timed = GetProcessTimes(h, &mut created, &mut exited, &mut kernel, &mut user).is_ok();
        let _ = CloseHandle(h);
        let Some(path) = path.filter(|_| timed) else {
            return false;
        };
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

/// 環境変数の秒数（無い・読めなければ既定）。巨大な値で Instant の足し算が溢れないよう上限で切る。
fn wait_secs(var: &str, default: u64, max: u64) -> u64 {
    std::env::var(var)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
        .min(max)
}

/// `POST /permission` で登録し、答えが出るまで `GET /permission/<id>` を問い合わせる。答えの JSON
/// （`state` が `pending` 以外）を返す。時間切れ・アプリが落ちた・id が消えたら None。
fn wait_for_state(disc: &Discovery, body: &str, wait: Duration) -> Option<serde_json::Value> {
    let (status, resp) = call(disc, "POST", "/permission", body)?;
    if status != 200 {
        return None;
    }
    let id = serde_json::from_str::<serde_json::Value>(&resp).ok()?["id"]
        .as_str()?
        .to_string();
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        std::thread::sleep(POLL_INTERVAL);
        let Some((200, resp)) = call(disc, "GET", &format!("/permission/{id}"), "") else {
            // アプリが落ちた・id が消えた
            return None;
        };
        let state = serde_json::from_str::<serde_json::Value>(&resp).ok()?;
        if state["state"].as_str()? != "pending" {
            return Some(state);
        }
    }
    None
}

fn wait_for_decision(disc: &Discovery, body: &str) -> Option<Decision> {
    // hook の timeout は 60 秒なので、それ以上は意味が無い
    let wait = wait_secs(
        "PET_PERMISSION_WAIT_SECS",
        DEFAULT_WAIT_SECS,
        DEFAULT_WAIT_SECS * 2,
    );
    let state = wait_for_state(disc, body, Duration::from_secs(wait))?;
    match state["state"].as_str()? {
        "allow" => Some(Decision::Allow),
        "deny" => Some(Decision::Deny {
            message: DENY_MESSAGE.to_string(),
        }),
        "ask" => Some(Decision::Ask),
        _ => None,
    }
}

/// 質問の答え。`allow` のときは応答の `answers`（`Reply` の並び）を読む。読めなければ None（`{}` になる）。
fn wait_for_answer(disc: &Discovery, body: &str) -> Option<AskAnswer> {
    let wait = wait_secs(
        "PET_ASK_WAIT_SECS",
        DEFAULT_ASK_WAIT_SECS,
        MAX_ASK_WAIT_SECS,
    );
    let state = wait_for_state(disc, body, Duration::from_secs(wait))?;
    match state["state"].as_str()? {
        "allow" => Some(AskAnswer::Answered(
            serde_json::from_value(state["answers"].clone()).ok()?,
        )),
        "deny" => Some(AskAnswer::Deny),
        "ask" => Some(AskAnswer::Ask),
        _ => None,
    }
}
