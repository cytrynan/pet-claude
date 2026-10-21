//! pet-hook の故障注入（design.md「検証」2）。ビルドした exe を実際に起動する。
//!
//! どの壊れ方でも「終了コード 0・すぐ終わる・承認なら `{}`」であること。
//! 発見ファイルは一時フォルダに置き、`PET_CLAUDE_DATA_DIR` で向ける（本物のアプリには触らない）。

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const PERMISSION: &str = r#"{"hook_event_name":"PermissionRequest","session_id":"s1","tool_name":"Bash","tool_input":{"command":"echo hi"}}"#;
const STOP: &str = r#"{"hook_event_name":"Stop","session_id":"s1"}"#;

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pet-hook-fault-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("runtime")).unwrap();
    d
}

/// 発見ファイルを書く。pid はこのテストのプロセス（hook は pid が生きていてアプリのときだけ繋ぐので、
/// `run_hook` が「アプリの実行ファイルのパス」をこのテストのパスに差し替える）。
fn write_discovery(dir: &Path, endpoint: &str, token: &str) {
    write_discovery_pid(dir, endpoint, token, std::process::id());
}

fn write_discovery_pid(dir: &Path, endpoint: &str, token: &str, pid: u32) {
    let text = format!(r#"{{"endpoint":"{endpoint}","token":"{token}","pid":{pid}}}"#);
    std::fs::write(dir.join("runtime").join("ipc.json"), text).unwrap();
}

fn this_exe_path() -> String {
    std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

/// (終了コード, 標準出力, かかった時間)
fn run_hook(dir: &Path, input: &[u8]) -> (i32, String, Duration) {
    run_hook_as(dir, input, &this_exe_path())
}

fn run_hook_as(dir: &Path, input: &[u8], app_path: &str) -> (i32, String, Duration) {
    let start = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_pet-hook"))
        .env("PET_CLAUDE_DATA_DIR", dir)
        .env("PET_CLAUDE_APP_PATH", app_path)
        .env("PET_PERMISSION_WAIT_SECS", "2")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    // 上限を超える入力は途中で読むのをやめる（書き込み側は broken pipe になる）ので失敗を無視する
    let _ = stdin.write_all(input);
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        start.elapsed(),
    )
}

// 起動そのものに 100ms 前後かかることがあるので、上限は design.md の値に起動の余裕を足す
const QUICK: Duration = Duration::from_millis(1500);

#[test]
fn app_not_running_permission_falls_back() {
    let dir = temp_dir("absent");
    let (code, out, took) = run_hook(&dir, PERMISSION.as_bytes());
    assert_eq!(code, 0);
    assert_eq!(out.trim(), "{}");
    assert!(took < QUICK, "{took:?}");
}

#[test]
fn app_not_running_event_is_silent() {
    let dir = temp_dir("absent-ev");
    let (code, out, took) = run_hook(&dir, STOP.as_bytes());
    assert_eq!(code, 0);
    assert_eq!(out, "");
    assert!(took < QUICK, "{took:?}");
}

#[test]
fn stale_discovery_port_closed() {
    let dir = temp_dir("stale");
    // 一度開けて閉じたポート＝誰も待っていない
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    write_discovery(&dir, &format!("127.0.0.1:{port}"), "t");
    let (code, out, took) = run_hook(&dir, PERMISSION.as_bytes());
    assert_eq!(code, 0);
    assert_eq!(out.trim(), "{}");
    assert!(took < QUICK, "{took:?}");
}

#[test]
fn wrong_token_gets_401_and_falls_back() {
    let dir = temp_dir("token");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    write_discovery(&dir, &listener.local_addr().unwrap().to_string(), "wrong");
    let server = std::thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let _ = s.write_all(
                b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            );
        }
    });
    let (code, out, took) = run_hook(&dir, PERMISSION.as_bytes());
    let _ = server.join();
    assert_eq!(code, 0);
    assert_eq!(out.trim(), "{}");
    assert!(took < QUICK, "{took:?}");
}

#[test]
fn server_accepts_but_never_answers() {
    let dir = temp_dir("hang");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    write_discovery(&dir, &listener.local_addr().unwrap().to_string(), "t");
    // 受け取るだけで何も返さない（5 秒つかんだまま）
    let server = std::thread::spawn(move || {
        if let Ok((s, _)) = listener.accept() {
            std::thread::sleep(Duration::from_secs(5));
            drop(s);
        }
    });
    let (code, out, took) = run_hook(&dir, PERMISSION.as_bytes());
    assert_eq!(code, 0);
    assert_eq!(out.trim(), "{}");
    // 読み取りの時間切れ 500ms＋起動
    assert!(took < QUICK, "{took:?}");
    drop(server);
}

#[test]
fn stale_discovery_hijacked_port_is_not_trusted() {
    // アプリが強制終了して発見ファイルが残り、そのポートで別のプロセスが待ち受けて「許可」を返す
    // （2026-10-21 検証係の指摘）。pid がアプリでなければ hook は繋がずに {} を返す
    let dir = temp_dir("hijack");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    write_discovery_pid(
        &dir,
        &listener.local_addr().unwrap().to_string(),
        "t",
        std::process::id(),
    );
    let (code, out, _) = run_hook_as(&dir, PERMISSION.as_bytes(), "C:/elsewhere/pet-app.exe");
    assert_eq!(code, 0);
    assert_eq!(out.trim(), "{}");
    // 一度も接続されていない
    assert!(listener.accept().is_err());
    // 死んだ pid も同じ
    write_discovery_pid(
        &dir,
        &listener.local_addr().unwrap().to_string(),
        "t",
        0xFFFF_FFF0,
    );
    let (_, out, _) = run_hook(&dir, PERMISSION.as_bytes());
    assert_eq!(out.trim(), "{}");
    assert!(listener.accept().is_err());
}

#[test]
fn reused_pid_is_not_trusted() {
    // 強制終了したアプリの pid を、後から生まれた別プロセスが引き当てた場合（2026-10-21 検証係の指摘）。
    // 発見ファイルの更新時刻をこのプロセスの起動より前に戻すと、「ファイルより後に生まれたプロセス」になる
    let dir = temp_dir("reuse");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    write_discovery(&dir, &listener.local_addr().unwrap().to_string(), "t");
    let file = std::fs::File::options()
        .write(true)
        .open(dir.join("runtime").join("ipc.json"))
        .unwrap();
    file.set_modified(std::time::SystemTime::now() - Duration::from_secs(24 * 3600))
        .unwrap();
    drop(file);
    let (code, out, _) = run_hook(&dir, PERMISSION.as_bytes());
    assert_eq!(code, 0);
    assert_eq!(out.trim(), "{}");
    assert!(listener.accept().is_err());
}

#[test]
fn server_drips_bytes_forever() {
    let dir = temp_dir("drip");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    write_discovery(&dir, &listener.local_addr().unwrap().to_string(), "t");
    // 読み取りの時間切れ（500ms）より短い間隔で 1 バイトずつ返し、閉じない（2026-10-21 検証係の指摘）
    let server = std::thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            for _ in 0..30 {
                if s.write_all(b"x").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    });
    let (code, out, took) = run_hook(&dir, PERMISSION.as_bytes());
    assert_eq!(code, 0);
    assert_eq!(out.trim(), "{}");
    // 応答全体の期限 1 秒＋起動
    assert!(took < Duration::from_millis(2500), "{took:?}");
    drop(server);
}

#[test]
fn broken_input_is_silent() {
    let dir = temp_dir("broken");
    for input in [
        &b"not json"[..],
        b"",
        b"{\"hook_event_name\":42}",
        &[0xff, 0xfe, 0x00],
    ] {
        let (code, out, took) = run_hook(&dir, input);
        assert_eq!(code, 0);
        assert_eq!(out, "", "input {input:?}");
        assert!(took < QUICK, "{took:?}");
    }
}

#[test]
fn oversized_input_is_silent() {
    let dir = temp_dir("big");
    let big = format!(
        r#"{{"hook_event_name":"Stop","session_id":"s1","pad":"{}"}}"#,
        "x".repeat(200_000)
    );
    let (code, out, took) = run_hook(&dir, big.as_bytes());
    assert_eq!(code, 0);
    assert_eq!(out, "");
    assert!(took < QUICK, "{took:?}");
}

#[test]
fn malformed_discovery_file() {
    let dir = temp_dir("baddisc");
    std::fs::write(dir.join("runtime").join("ipc.json"), "{ not json").unwrap();
    let (code, out, _) = run_hook(&dir, PERMISSION.as_bytes());
    assert_eq!(code, 0);
    assert_eq!(out.trim(), "{}");
}
