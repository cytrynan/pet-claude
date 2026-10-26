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

// ---------------------------------------------------------------------------
// 質問（AskUserQuestion）。原本: .steering/20261027-ask-question/brief.md A5
// ---------------------------------------------------------------------------

const ASK: &str = r#"{"hook_event_name":"PreToolUse","session_id":"s1","tool_name":"AskUserQuestion","tool_input":{"questions":[{"question":"どの色？","header":"色","options":[{"label":"Red","description":"赤"},{"label":"Blue","description":"青"}],"multiSelect":false}]}}"#;

/// `--ask` 付きで起こす（質問を扱う登録。install.rs が付ける引数と同じ）。
fn run_hook_ask(dir: &Path, input: &[u8]) -> (i32, String, Duration) {
    let start = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_pet-hook"))
        .arg("--pet-claude-managed")
        .arg("--ask")
        .env("PET_CLAUDE_DATA_DIR", dir)
        .env("PET_CLAUDE_APP_PATH", this_exe_path())
        .env("PET_ASK_WAIT_SECS", "3")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let _ = stdin.write_all(input);
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        start.elapsed(),
    )
}

/// 偽のアプリ: `POST /permission` に id を返し、`GET /permission/<id>` に `state_body` を返す。
/// 受けた要求の本文（POST の分）を返す。
fn fake_app(listener: TcpListener, state_body: &'static str) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut posted = String::new();
        for _ in 0..4 {
            let Ok((mut s, _)) = listener.accept() else {
                break;
            };
            let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            while let Ok(n) = s.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if let Ok(req) = pet_core::ipc::parse_request(&buf) {
                    let body = if req.method == "POST" {
                        posted = String::from_utf8_lossy(&req.body).into_owned();
                        r#"{"id":"q1"}"#
                    } else {
                        state_body
                    };
                    let _ = s.write_all(&pet_core::ipc::format_response(200, body));
                    break;
                }
            }
            if !posted.is_empty() && buf.starts_with(b"GET") {
                break;
            }
        }
        posted
    })
}

#[test]
fn ask_without_flag_outputs_nothing() {
    // `*` の非同期登録から来た分: アプリが居ても何も送らず、何も書かない
    let dir = temp_dir("ask-noflag");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    write_discovery(&dir, &listener.local_addr().unwrap().to_string(), "t");
    let (code, out, took) = run_hook(&dir, ASK.as_bytes());
    assert_eq!(code, 0);
    assert_eq!(out, "");
    assert!(took < QUICK, "{took:?}");
    assert!(listener.accept().is_err(), "接続してはいけない");
}

#[test]
fn ask_app_absent_falls_back_to_the_usual_screen() {
    let dir = temp_dir("ask-absent");
    let (code, out, took) = run_hook_ask(&dir, ASK.as_bytes());
    assert_eq!(code, 0);
    assert_eq!(out.trim(), "{}");
    assert!(took < QUICK, "{took:?}");
}

#[test]
fn ask_answer_from_app_becomes_updated_input() {
    let dir = temp_dir("ask-answer");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    write_discovery(&dir, &listener.local_addr().unwrap().to_string(), "t");
    let server = fake_app(
        listener,
        r#"{"state":"allow","answers":[{"chosen":["Blue"]}]}"#,
    );
    let (code, out, _) = run_hook_ask(&dir, ASK.as_bytes());
    let posted = server.join().unwrap();
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    let hs = &v["hookSpecificOutput"];
    assert_eq!(hs["hookEventName"], "PreToolUse");
    assert_eq!(hs["permissionDecision"], "allow");
    assert_eq!(hs["updatedInput"]["answers"]["どの色？"], "Blue");
    assert_eq!(
        hs["updatedInput"]["questions"][0]["options"][1]["label"],
        "Blue"
    );
    // アプリへは質問の中身と来た先を送っている（プロンプトの本文は無い）
    let sent: serde_json::Value = serde_json::from_str(&posted).unwrap();
    assert_eq!(sent["kind"], "ask_question");
    assert_eq!(sent["questions"][0]["question"], "どの色？");
    assert!(sent.get("origin").is_some());
}

#[test]
fn ask_deny_and_ask_and_broken_answers() {
    for (state, expect_decision) in [
        (r#"{"state":"deny"}"#, Some("deny")),
        (r#"{"state":"ask"}"#, None),
        // 選択肢に無いラベル・形の違う答えは {}（いつもの画面）
        (
            r#"{"state":"allow","answers":[{"chosen":["Purple"]}]}"#,
            None,
        ),
        (r#"{"state":"allow","answers":"x"}"#, None),
        (r#"{"state":"allow"}"#, None),
    ] {
        let dir = temp_dir("ask-states");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        write_discovery(&dir, &listener.local_addr().unwrap().to_string(), "t");
        let server = fake_app(listener, state);
        let (code, out, _) = run_hook_ask(&dir, ASK.as_bytes());
        let _ = server.join();
        assert_eq!(code, 0);
        match expect_decision {
            Some(d) => {
                let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
                assert_eq!(v["hookSpecificOutput"]["permissionDecision"], d, "{state}");
                assert!(
                    v["hookSpecificOutput"]["permissionDecisionReason"]
                        .as_str()
                        .is_some_and(|r| r.contains("pet-claude"))
                );
            }
            None => assert_eq!(out.trim(), "{}", "{state}"),
        }
    }
}

#[test]
fn ask_times_out_to_the_usual_screen() {
    let dir = temp_dir("ask-timeout");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    write_discovery(&dir, &listener.local_addr().unwrap().to_string(), "t");
    // ずっと pending（PET_ASK_WAIT_SECS=3 で諦める）
    let server = std::thread::spawn(move || {
        for _ in 0..40 {
            let Ok((mut s, _)) = listener.accept() else {
                break;
            };
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let body = if buf.starts_with(b"POST") {
                r#"{"id":"q1"}"#
            } else {
                r#"{"state":"pending"}"#
            };
            let _ = s.write_all(&pet_core::ipc::format_response(200, body));
        }
    });
    let (code, out, took) = run_hook_ask(&dir, ASK.as_bytes());
    assert_eq!(code, 0);
    assert_eq!(out.trim(), "{}");
    assert!(
        took >= Duration::from_secs(3) && took < Duration::from_secs(6),
        "{took:?}"
    );
    drop(server);
}

#[test]
fn permission_request_for_ask_user_question_is_left_to_claude_code() {
    // 質問の hook が「決めない」を返した後、auto mode は AskUserQuestion の PermissionRequest を起こす
    // （2026-10-27 実測）。それは画面の質問そのものなので、アプリに送らず {} で任せる（カードを出さない）
    let dir = temp_dir("ask-perm");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    write_discovery(&dir, &listener.local_addr().unwrap().to_string(), "t");
    let input = r#"{"hook_event_name":"PermissionRequest","session_id":"s1","tool_name":"AskUserQuestion","tool_input":{"questions":[]}}"#;
    let (code, out, took) = run_hook(&dir, input.as_bytes());
    assert_eq!(code, 0);
    assert_eq!(out.trim(), "{}");
    assert!(took < QUICK, "{took:?}");
    assert!(listener.accept().is_err(), "接続してはいけない");
}
