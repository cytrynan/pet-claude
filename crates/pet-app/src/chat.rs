//! バーのチャットを Claude につなぐ（Phase 3 前半。設計 `.steering/20261021-phase3-chat/design.md`）。
//!
//! 送るたびに `claude -p` を子プロセスで 1 回起こし、答えの JSON を読んで終わる（使っていない間は何も常駐しない）。
//! 引数・環境・出力の解析は `pet_core::chat`（純粋な関数）、ここは OS とスレッドの側だけを持つ。
//!
//! - 同時に 1 本だけ（待ち中の送信は断る）。`TIMEOUT_SECS` で子プロセスを止めて時間切れ
//! - 子は送信ごとの Job Object（終わったら閉じると中身ごと止まる）に入れる。pet-app が落ちても
//!   OS がハンドルを閉じるので道連れになる（A7）
//! - 世代番号: 「新しいチャット」・終了で世代を進める。古い世代の答えは捨てる
//! - ログには字数・秒数・終了コード・is_error だけ。本文・答え・鍵は書かない（N4）

use std::io::{Read, Write};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use pet_core::chat::{self as core_chat, AuthMode, ChatOutcome};
use tauri::AppHandle;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

use crate::log::{data_dir, log};

/// ペットとしての話し方（Claude Code 既定のコーディング用の指示を丸ごと置き換える。設計 §2）。
const SYSTEM_PROMPT: &str = "あなたは Windows のデスクトップにいる小さなペットです。持ち主と気軽に話します。\n\
日本語で、短く（目安 2〜3 文）答えてください。\n\
コード・コマンド・URL・ファイルのパスは書かないでください（吹き出しに出すため）。\n\
ファイルを読んだり、コマンドを実行したり、Web を調べたりする道具は持っていません。頼まれたら、できないと短く伝えてください。";

/// 標準出力・標準エラーを読む上限（暴走した出力で pet-app のメモリを食わないように。答えは数 KB）。
const MAX_OUTPUT_BYTES: u64 = 1 << 20;
/// 子プロセスの終わりを見る間隔。
const POLL: Duration = Duration::from_millis(100);
/// 行動ロジックに渡すセッション名（hooks の出来事と混ざらない固定の名前）。
const BEHAVIOR_SESSION: &str = "pet-chat";

/// 世代番号。送信のたびに 1 進め、「新しいチャット」・終了でも進める（古い答えを捨てる印）。
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// 走っている子の Job Object（世代, ハンドルの値）。止めるときと終わったときに同じ鍵で取り合う。
/// 送信の受け付け（メインスレッド）でハンドル 0 の印を置き、子を起こしたスレッドが本物のハンドルに差し替える。
/// 受け付けの時点で置くのは、子を起こす前に「新しいチャット」や 2 通目が来ても 1 本だけを守るため
/// （メインスレッドが詰まって手順がまとめて走ったとき、2 本同時に起き、止めるも空振りした。2026-10-21 故障注入で発見）。
static RUNNING: Mutex<Option<(u64, isize)>> = Mutex::new(None);
/// 会話の続きに使う `session_id`（`--resume`）。「新しいチャット」で消す。
static SESSION: Mutex<Option<String>> = Mutex::new(None);

/// チャット欄の送信。待たずに戻り、答えは後で吹き出しとチャット欄に出す。戻り値は入力欄に出す一言。
pub fn submit(app: AppHandle, text: String, auth: AuthMode) -> String {
    let text = text.trim().to_string();
    if text.is_empty() {
        return String::new();
    }
    // 空きの確認と予約を同じ鍵の中で行う（確認と予約の間に別の送信が入らないように）
    let generation = {
        let Ok(mut r) = RUNNING.lock() else {
            return "送れませんでした".into();
        };
        if r.is_some() {
            return "まだ考え中です".into();
        }
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        *r = Some((generation, 0));
        generation
    };
    let resume = SESSION.lock().ok().and_then(|s| s.clone());
    log(format!(
        "chat send ({} 字, auth={}, resume={})",
        text.chars().count(),
        auth.as_str(),
        resume.is_some()
    ));
    crate::pet_window::chat_thinking(BEHAVIOR_SESSION);
    let spawned = std::thread::Builder::new()
        .name("chat".into())
        .spawn(move || {
            let started = Instant::now();
            let outcome = run(generation, &text, auth, resume.as_deref());
            // 子を起こす前に失敗して戻った時も、予約を外す（外さないと以後ずっと「考え中」になる）
            release(generation);
            let _ = app.run_on_main_thread(move || finish(generation, outcome, started));
        });
    match spawned {
        Ok(_) => "考え中…".into(),
        Err(e) => {
            release(generation);
            log(format!("chat のスレッドを作れない: {e}"));
            crate::pet_window::chat_finished(BEHAVIOR_SESSION, false);
            "送れませんでした".into()
        }
    }
}

/// 「新しいチャット」: 会話を忘れ、待ち中の子があれば止める。
pub fn new_chat() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut s) = SESSION.lock() {
        *s = None;
    }
    // 止めた時は作業中の動きを終わらせる（行動ロジックに「止めた」は無いので Done＝小さく跳ねる）
    if cancel_running() {
        crate::pet_window::chat_finished(BEHAVIOR_SESSION, true);
    }
    log("chat: new conversation");
}

/// アプリの終了で呼ぶ。待ち中の子を止める（Job Object のハンドルが閉じても止まるが、先に明示して止める）。
pub fn shutdown() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    cancel_running();
}

/// 待ち中かどうか（チャット欄を開いた時に入力欄の状態を合わせる）。
pub fn busy() -> bool {
    RUNNING.lock().map(|r| r.is_some()).unwrap_or(false)
}

/// 走っている子を Job ごと止める。止めたら true。
fn cancel_running() -> bool {
    let Ok(mut r) = RUNNING.lock() else {
        return false;
    };
    match r.take() {
        Some((generation, job)) => {
            // job が 0 ならまだ子を起こしていない（起こしたスレッドが、予約が消えたのを見て自分で止める）
            if job != 0 {
                // SAFETY: job は RUNNING に入っている間は閉じられない（閉じる側も同じ鍵で取り出してから閉じる）
                let _ = unsafe { TerminateJobObject(HANDLE(job as *mut _), 1) };
            }
            log(format!("chat: cancelled generation {generation}"));
            true
        }
        None => false,
    }
}

/// 答えをメインスレッドで受ける。古い世代なら捨てる。
fn finish(generation: u64, outcome: ChatOutcome, started: Instant) {
    let secs = started.elapsed().as_secs_f32();
    if GENERATION.load(Ordering::SeqCst) != generation {
        log(format!("chat: discarded stale answer ({secs:.1}s)"));
        return;
    }
    match outcome {
        ChatOutcome::Answer { text, session_id } => {
            log(format!(
                "chat answer in {secs:.1}s ({} 字)",
                text.chars().count()
            ));
            if let (Some(id), Ok(mut s)) = (session_id, SESSION.lock()) {
                *s = Some(id);
            }
            crate::pet_window::chat_finished(BEHAVIOR_SESSION, true);
            crate::ui::show_chat_answer(&text);
        }
        ChatOutcome::Failed { reason, session_id } => {
            log(format!("chat failed in {secs:.1}s"));
            // 失敗でも session_id が返れば会話は続けられる（上限・鍵違いでも id は返る。evidence E2・E11）
            if let (Some(id), Ok(mut s)) = (session_id, SESSION.lock()) {
                *s = Some(id);
            }
            crate::pet_window::chat_finished(BEHAVIOR_SESSION, false);
            crate::ui::show_chat_failure(&reason);
        }
    }
}

fn failed(reason: &str) -> ChatOutcome {
    ChatOutcome::Failed {
        reason: reason.into(),
        session_id: None,
    }
}

/// 子プロセスを起こして答えを待つ（チャットのスレッドで走る）。
fn run(generation: u64, text: &str, auth: AuthMode, resume: Option<&str>) -> ChatOutcome {
    let Some(exe) = claude_exe() else {
        return failed("Claude Code が見つかりません");
    };
    let key = match auth {
        AuthMode::ApiKey => crate::cred::load_key(),
        AuthMode::Subscription => None,
    };
    let parent: Vec<(String, String)> = std::env::vars().collect();
    let Ok(env) = core_chat::child_env(auth, &parent, key.as_deref()) else {
        return failed("API キーが保存されていません（設定で入れてください）");
    };
    let dir = data_dir().join("chat");
    let work = dir.join("work");
    let prompt_file = dir.join("system-prompt.txt");
    if std::fs::create_dir_all(&work).is_err()
        || std::fs::write(&prompt_file, SYSTEM_PROMPT).is_err()
    {
        return failed("作業フォルダを作れません");
    }

    let mut cmd = Command::new(&exe);
    cmd.args(core_chat::child_args(
        &prompt_file.to_string_lossy(),
        resume,
    ))
    .env_clear()
    .envs(env)
    .current_dir(&work)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .creation_flags(CREATE_NO_WINDOW.0);
    let job = match new_kill_on_close_job() {
        Ok(j) => j,
        Err(e) => {
            log(format!("chat: Job Object を作れない: {e}"));
            return failed("Claude Code を起こせませんでした");
        }
    };
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            // SAFETY: 自分で作ったハンドルを 1 回だけ閉じる
            let _ = unsafe { CloseHandle(job) };
            log(format!("chat: 起動できない: {e}"));
            return failed("Claude Code を起こせませんでした");
        }
    };
    // SAFETY: child のハンドルは child が生きている間有効
    if let Err(e) =
        unsafe { AssignProcessToJobObject(job, HANDLE(child.as_raw_handle() as *mut _)) }
    {
        // 入れられなくても止める手段（child.kill）はあるので続ける。孫は道連れにならない
        log(format!("chat: Job Object に入れられない: {e}"));
    }
    // 予約に本物のハンドルを入れる。予約が消えていたら（起こす前に止められた）、すぐ止めて戻る
    let still_reserved = match RUNNING.lock() {
        Ok(mut r) if r.map(|(g, _)| g) == Some(generation) => {
            *r = Some((generation, job.0 as isize));
            true
        }
        _ => false,
    };
    if !still_reserved {
        let _ = child.kill();
        let _ = child.wait();
        close_job(generation, job);
        return failed("止めました");
    }

    // 本文は標準入力で渡す（コマンドラインに載せない＝引用符の罠とプロセス一覧からの露出を避ける）
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    let out = child.stdout.take().map(read_capped);
    let err = child.stderr.take().map(read_capped);

    let deadline = Instant::now() + timeout();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {}
            Err(_) => break None,
        }
        // 止められた（新しいチャット・終了）なら、答えを待たずに抜ける
        let still_ours = RUNNING
            .lock()
            .map(|r| r.map(|(g, _)| g) == Some(generation))
            .unwrap_or(false);
        if !still_ours {
            let _ = child.kill();
            let _ = child.wait();
            close_job(generation, job);
            return failed("止めました");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            close_job(generation, job);
            log(format!(
                "chat: timed out after {}s",
                timeout().as_secs_f32()
            ));
            return failed(&format!("時間切れ（{} 秒）", timeout().as_secs()));
        }
        std::thread::sleep(POLL);
    };
    close_job(generation, job);
    let stdout = out.and_then(|h| h.join().ok()).unwrap_or_default();
    let stderr = err.and_then(|h| h.join().ok()).unwrap_or_default();
    let code = status.and_then(|s| s.code());
    let outcome = core_chat::parse_output(code, &stdout, &stderr);
    log(format!(
        "chat: exit={} is_error={}",
        code.map_or("none".into(), |c| c.to_string()),
        matches!(outcome, ChatOutcome::Failed { .. })
    ));
    outcome
}

/// 自分の世代の予約が残っていれば外す（何度呼んでもよい）。
fn release(generation: u64) {
    if let Ok(mut r) = RUNNING.lock()
        && r.map(|(g, _)| g) == Some(generation)
    {
        *r = None;
    }
}

/// RUNNING から自分の世代を外してから Job を閉じる（閉じると中に残った孫も止まる）。
fn close_job(generation: u64, job: HANDLE) {
    if let Ok(mut r) = RUNNING.lock()
        && r.map(|(g, _)| g) == Some(generation)
    {
        *r = None;
    }
    // SAFETY: このスレッドが作ったハンドルを 1 回だけ閉じる。cancel_running は鍵の中でしか触らず、
    // 上で RUNNING から外した（か、既に cancel が外した）後なので、もう誰も使わない
    let _ = unsafe { CloseHandle(job) };
}

/// 閉じると中のプロセスが全部止まる Job Object。
fn new_kill_on_close_job() -> windows::core::Result<HANDLE> {
    // SAFETY: 名前なし・既定のセキュリティで作り、設定の構造体はこの関数の中で生きている
    unsafe {
        let job = CreateJobObjectW(None, None)?;
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if let Err(e) = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&raw const info).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) {
            let _ = CloseHandle(job);
            return Err(e);
        }
        Ok(job)
    }
}

/// 出力を上限まで読む別スレッド（パイプが詰まって子が止まらないように、待つのと並行して読む）。
fn read_capped(r: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut r = r;
        let _ = r.by_ref().take(MAX_OUTPUT_BYTES).read_to_end(&mut buf);
        // 上限を超えた残りは捨てながら読み切る（読まないと子がパイプで止まる）
        let _ = std::io::copy(&mut r, &mut std::io::sink());
        buf
    })
}

/// 試験用の差し替えを許すか（デバッグビルドで、置き場所を `PET_CLAUDE_DATA_DIR` で差し替えた時だけ。
/// main.rs の `switch_pet_for_tests` と同じ条件）。
fn test_overrides() -> bool {
    cfg!(debug_assertions) && std::env::var_os("PET_CLAUDE_DATA_DIR").is_some()
}

/// 時間切れまでの長さ。試験では `PET_CHAT_TIMEOUT_SECS` で縮められる。
fn timeout() -> Duration {
    let secs = test_overrides()
        .then(|| std::env::var("PET_CHAT_TIMEOUT_SECS").ok())
        .flatten()
        .and_then(|s| s.parse().ok())
        .unwrap_or(core_chat::TIMEOUT_SECS);
    Duration::from_secs(secs)
}

/// `claude` の実行ファイル。試験では `PET_CLAUDE_EXE` で偽物に差し替えられる。
fn claude_exe() -> Option<PathBuf> {
    if test_overrides()
        && let Some(p) = std::env::var_os("PET_CLAUDE_EXE")
    {
        return Some(PathBuf::from(p));
    }
    let dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
    core_chat::find_claude(&dirs, home.as_deref(), &|p| p.is_file())
}

/// 設定画面の「いまの接続を確かめる」: チャットと同じ環境で `claude auth status` を走らせる（数秒かかるので別スレッドから呼ぶ）。
pub fn auth_status(auth: AuthMode) -> String {
    let Some(exe) = claude_exe() else {
        return "Claude Code が見つかりません".into();
    };
    let key = match auth {
        AuthMode::ApiKey => crate::cred::load_key(),
        AuthMode::Subscription => None,
    };
    let parent: Vec<(String, String)> = std::env::vars().collect();
    let Ok(env) = core_chat::child_env(auth, &parent, key.as_deref()) else {
        return "API キーが保存されていません".into();
    };
    let out = Command::new(exe)
        .args(["auth", "status"])
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW.0)
        .output();
    match out {
        Ok(o) => core_chat::auth_status_label(&o.stdout),
        Err(_) => "確かめられませんでした".into(),
    }
}
