//! hook からの出来事の受け口。`127.0.0.1:0` で待ち受ける別スレッド（Phase 2）。
//!
//! - 起動時に発見ファイル `runtime/ipc.json`（`Discovery`）を一時ファイル＋rename で書き、終了時に消す
//! - token の合わない要求・`Origin` 付きの要求は 401（`ipc::is_authorized`）
//! - `POST /event`: 出来事をメインスレッドへ回す（`run_on_main_thread` を別スレッドから呼ぶのでキューに積まれる）
//! - `POST /permission`: 承認の表に Pending で入れ、吹き出しに出す。`{"id": ...}` を返す。質問
//!   （kind `ask_question`）も同じ表に入れる（期限だけ長い）
//! - `GET /permission/<id>`: 状態を返す（質問の答えは `answers` に `Reply` の並び）。答えが出ていれば、返した後に表から消す
//!
//! 承認の表は受け口のスレッドとメインスレッドの両方から触るので `Mutex` に置く。

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use pet_core::ask::Reply;
use pet_core::ipc::{self, Discovery, ParseError, PermissionState, WireEvent};
use tauri::AppHandle;

use crate::log::{data_dir, log};

/// 答えを待つ承認をこの時間で捨てる（hook 側の待ちは最大 55 秒）。
const PERMISSION_TTL: Duration = Duration::from_secs(120);
/// 質問の期限（hook 側の待ちは最大 295 秒・登録する hook の timeout は 300 秒）。
const ASK_TTL: Duration = Duration::from_secs(300);
/// hook はこの間隔より短く（300ms ごとに）問い合わせる。これだけ問い合わせが無ければ、hook は
/// あきらめた・殺された（ターミナルで答えた）とみなしてカードを下げる。
const ABANDONED_AFTER: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(1);
/// 1 つの要求を読み切る期限（hook の要求は数百バイトで、数 ms で届く）。
const REQUEST_DEADLINE: Duration = Duration::from_secs(2);
/// 同時に処理する接続の上限（hook は 1 回に 1 本。超えた分はすぐ切る）。
const MAX_CONNECTIONS: usize = 16;
static CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

struct Entry {
    state: PermissionState,
    created: Instant,
    /// 最後に hook が問い合わせた時刻（登録時は登録の時刻）
    polled: Instant,
    /// 期限（承認 120 秒・質問 300 秒）
    ttl: Duration,
    /// 質問の答え（`state` が Allow のときだけ）
    answers: Option<Vec<Reply>>,
}

impl Entry {
    fn is_live(&self, now: Instant) -> bool {
        is_live(self.created, self.polled, now, self.ttl)
    }
}

/// まだ答えを受け付けてよいか（期限内で、hook が問い合わせ続けている）。
fn is_live(created: Instant, polled: Instant, now: Instant, ttl: Duration) -> bool {
    now.duration_since(created) < ttl && now.duration_since(polled) < ABANDONED_AFTER
}

static PERMISSIONS: LazyLock<Mutex<HashMap<String, Entry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn discovery_path() -> std::path::PathBuf {
    data_dir().join("runtime").join("ipc.json")
}

/// 乱数を 16 進で（token と承認の id に使う）。取れなければ None（予測できる値で代用しない）。
fn random_hex(bytes: usize) -> Option<String> {
    use windows::Win32::Security::Cryptography::{
        BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
    };
    let mut buf = vec![0u8; bytes];
    // SAFETY: 出力先は buf（長さ bytes）
    let ok = unsafe { BCryptGenRandom(None, &mut buf, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    if ok.is_err() {
        // 時刻などで代用すると token を推測される（2026-10-21 検証係の指摘）。連携を開かない側に倒す
        log("BCryptGenRandom に失敗");
        return None;
    }
    Some(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// 受け口を開いて発見ファイルを書く。失敗したら連携なしで続ける（ペットは動く）。
pub fn start(app: AppHandle) {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(l) => l,
        Err(e) => {
            log(format!("受け口を開けない（hooks 連携なしで続ける）: {e}"));
            return;
        }
    };
    let Ok(addr) = listener.local_addr() else {
        return;
    };
    let Some(token) = random_hex(32) else {
        log("乱数が取れないので受け口を開かない（hooks 連携なしで続ける）");
        return;
    };
    let disc = Discovery {
        endpoint: addr.to_string(),
        token: token.clone(),
        pid: std::process::id(),
    };
    if let Err(e) = write_discovery(&disc) {
        log(format!("発見ファイルを書けない: {e}"));
        return;
    }
    log(format!("ipc listening on {addr}"));
    let spawned = std::thread::Builder::new()
        .name("pet-ipc".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                // 1 接続ずつ順に処理すると、ヘッダを少しずつ送る接続 1 本で他の hook が全部待たされる
                // （2026-10-21 検証係の指摘）。接続ごとにスレッドを分け、同時の数に上限を置く
                if CONNECTIONS.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                    CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
                    continue;
                }
                let (token, app) = (token.clone(), app.clone());
                let spawned = std::thread::Builder::new().spawn(move || {
                    handle(stream, &token, &app);
                    CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
                });
                if spawned.is_err() {
                    CONNECTIONS.fetch_sub(1, Ordering::SeqCst);
                }
            }
        });
    if let Err(e) = spawned {
        log(format!("受け口のスレッドを作れない: {e}"));
    }
}

fn write_discovery(disc: &Discovery) -> std::io::Result<()> {
    let path = discovery_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(disc).unwrap_or_default())?;
    std::fs::rename(&tmp, &path)
}

/// 終了時に発見ファイルを消す（hook が死んだ受け口へ繋ぎに行かないように）。
pub fn stop() {
    let _ = std::fs::remove_file(discovery_path());
}

fn handle(mut stream: TcpStream, token: &str, app: &AppHandle) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(READ_TIMEOUT));
    // 読み取りごとの時間切れに加えて、要求全体にも期限を置く（少しずつ送り続ける接続を切る）
    let deadline = Instant::now() + REQUEST_DEADLINE;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let req = loop {
        match ipc::parse_request(&buf) {
            Ok(r) => break Ok(r),
            Err(ParseError::Incomplete) => {}
            Err(e) => break Err(e),
        }
        // 読み取りの待ちを期限の残りで切り、読んだ後にも期限を見る（期限を過ぎて完成した要求は受け付けない）
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero()
            || stream
                .set_read_timeout(Some(left.min(READ_TIMEOUT)))
                .is_err()
        {
            break Err(ParseError::Incomplete);
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break Err(ParseError::Incomplete),
            Ok(_) if Instant::now() >= deadline => break Err(ParseError::Incomplete),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    let (status, body) = match req {
        Err(ParseError::TooLarge) => (413, r#"{"error":"too large"}"#.to_string()),
        Err(_) => (400, r#"{"error":"bad request"}"#.to_string()),
        Ok(r) if !ipc::is_authorized(&r, token) => (401, r#"{"error":"unauthorized"}"#.to_string()),
        Ok(r) => route(&r, app),
    };
    let _ = stream.write_all(&ipc::format_response(status, &body));
}

fn route(req: &ipc::Request, app: &AppHandle) -> (u16, String) {
    match (req.method.as_str(), req.path.as_str()) {
        ("POST", "/event") => match serde_json::from_slice::<WireEvent>(&req.body) {
            Ok(ev) => {
                let _ = app.run_on_main_thread(move || crate::pet_window::on_wire_event(&ev));
                (200, r#"{"ok":true}"#.to_string())
            }
            Err(_) => (400, r#"{"error":"bad event"}"#.to_string()),
        },
        ("POST", "/permission") => match serde_json::from_slice::<WireEvent>(&req.body) {
            Ok(ev) => {
                let Some(id) = random_hex(8) else {
                    return (500, r#"{"error":"no randomness"}"#.to_string());
                };
                // 質問は人が読んで考えるので期限を長く取る
                let ttl = if ev.kind == "ask_question" {
                    ASK_TTL
                } else {
                    PERMISSION_TTL
                };
                {
                    let mut table = PERMISSIONS.lock().unwrap_or_else(|p| p.into_inner());
                    table.retain(|_, e| e.created.elapsed() < e.ttl);
                    let now = Instant::now();
                    table.insert(
                        id.clone(),
                        Entry {
                            state: PermissionState::Pending,
                            created: now,
                            polled: now,
                            ttl,
                            answers: None,
                        },
                    );
                }
                let shown = id.clone();
                let _ = app.run_on_main_thread(move || {
                    crate::pet_window::on_wire_event(&ev);
                    crate::ui::show_permission(shown, ev);
                });
                (200, serde_json::json!({ "id": id }).to_string())
            }
            Err(_) => (400, r#"{"error":"bad event"}"#.to_string()),
        },
        ("GET", path) if path.starts_with("/permission/") => {
            let id = &path["/permission/".len()..];
            let mut table = PERMISSIONS.lock().unwrap_or_else(|p| p.into_inner());
            let now = Instant::now();
            match table.get_mut(id) {
                // 期限切れ・放置（5 秒問い合わせが無かった）は知らない id と同じ（hook は決めずに終わる）。
                // 放置された承認は吹き出しから下げてあるので、ここで生き返らせない（2026-10-21 検証係の指摘）
                Some(e) if e.state == PermissionState::Pending && !e.is_live(now) => {
                    table.remove(id);
                    (404, r#"{"error":"expired"}"#.to_string())
                }
                Some(e) => {
                    e.polled = now;
                    let state = e.state;
                    let mut body = serde_json::json!({ "state": state });
                    if let Some(answers) = &e.answers {
                        body["answers"] = serde_json::json!(answers);
                    }
                    if state != PermissionState::Pending {
                        table.remove(id);
                    }
                    (200, body.to_string())
                }
                None => (404, r#"{"error":"unknown id"}"#.to_string()),
            }
        }
        _ => (404, r#"{"error":"not found"}"#.to_string()),
    }
}

/// 吹き出しのボタン（または試験用の自動応答）から呼ぶ。待っている承認に答えを入れる。
/// 返り値: その id が待っていたか（もう答えた・期限切れなら false）。
pub fn decide(id: &str, state: PermissionState) -> bool {
    set_state(id, state, None)
}

/// 質問に全部答えた。`replies` は質問と同じ順（hook が `ask_output` で検査してから出力する）。
pub fn decide_with_answers(id: &str, replies: Vec<Reply>) -> bool {
    set_state(id, PermissionState::Allow, Some(replies))
}

fn set_state(id: &str, state: PermissionState, answers: Option<Vec<Reply>>) -> bool {
    let mut table = PERMISSIONS.lock().unwrap_or_else(|p| p.into_inner());
    match table.get_mut(id) {
        Some(e) if e.state == PermissionState::Pending && e.is_live(Instant::now()) => {
            e.state = state;
            e.answers = answers;
            true
        }
        _ => false,
    }
}

/// まだ答えていない、生きている承認か（期限切れ・hook が問い合わせをやめたものは false）。
pub fn is_pending(id: &str) -> bool {
    PERMISSIONS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(id)
        .is_some_and(|e| e.state == PermissionState::Pending && e.is_live(Instant::now()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_only_within_ttl_and_while_polled() {
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        assert!(is_live(t0, t0, at(1), PERMISSION_TTL));
        // hook が 5 秒問い合わせない（あきらめた・殺された）
        assert!(!is_live(t0, t0, at(5), PERMISSION_TTL));
        // 問い合わせ続けていても 120 秒で終わり
        assert!(is_live(t0, at(118), at(119), PERMISSION_TTL));
        assert!(!is_live(t0, at(119), at(120), PERMISSION_TTL));
        // 質問は 300 秒まで（5 秒の放置の判定は同じ）
        assert!(is_live(t0, at(298), at(299), ASK_TTL));
        assert!(!is_live(t0, at(299), at(300), ASK_TTL));
        assert!(!is_live(t0, t0, at(5), ASK_TTL));
    }

    #[test]
    fn random_hex_has_requested_length() {
        assert_eq!(random_hex(32).map(|s| s.len()), Some(64));
        assert_ne!(random_hex(8), random_hex(8));
    }
}
