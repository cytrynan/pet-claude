//! バーのチャットを Claude につなぐ（Phase 3 前半。設計 `.steering/20261021-phase3-chat/design.md`）。
//!
//! 送るたびに `claude -p` を子プロセスで 1 回起こし、答えの JSON を読んで終わる（使っていない間は何も常駐しない）。
//! 引数・環境・出力の解析は `pet_core::chat`（純粋な関数）、ここは OS とスレッドの側だけを持つ。
//!
//! - 同時に 1 本だけ（待ち中の送信は断る）。`TIMEOUT_SECS` で子プロセスを止めて時間切れ
//! - 子は止めた状態で起こして送信ごとの Job Object（閉じると中身ごと止まる）に入れてから動かす。
//!   pet-app が落ちても OS がハンドルを閉じるので道連れになる（A7）
//! - 世代番号: 「新しいチャット」・終了で世代を進める。古い世代の答えは捨てる
//! - ログには字数・秒数・終了コード・is_error だけ。本文・答え・鍵は書かない（N4）

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use pet_core::chat::{self as core_chat, AuthMode, ChatOutcome};
use tauri::AppHandle;

use crate::child::{ChildOutput, ChildRun, Stop, run_child, terminate_job, test_overrides};
use crate::log::{data_dir, log};

/// 子を起こせなかった時の一言。
const START_FAILED: &str = "Claude Code を起こせませんでした";

/// ペットとしての話し方（Claude Code 既定のコーディング用の指示を丸ごと置き換える。設計 §2）。
const SYSTEM_PROMPT: &str = "あなたは Windows のデスクトップにいる小さなペットです。持ち主と気軽に話します。\n\
日本語で、短く（目安 2〜3 文）答えてください。\n\
コード・コマンド・URL・ファイルのパスは書かないでください（吹き出しに出すため）。\n\
ファイルを読んだり、コマンドを実行したり、Web を調べたりする道具は持っていません。頼まれたら、できないと短く伝えてください。";

/// 「確かめる」（`claude auth status`）の時間切れ。普段は 1 秒ほどで返る。
const AUTH_STATUS_TIMEOUT: Duration = Duration::from_secs(15);
/// 行動ロジックに渡すセッション名（hooks の出来事と混ざらない固定の名前）。
pub const BEHAVIOR_SESSION: &str = "pet-chat";

/// 世代番号。送信のたびに 1 進め、「新しいチャット」・終了でも進める（古い答えを捨てる印）。
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// 走っている送信の予約（世代, Job Object のハンドルの値。子を起こす前と終わった後は 0）。
/// - 受け付け（メインスレッド）で置く。子を起こす前に「新しいチャット」や 2 通目が来ても 1 本だけを守るため
///   （メインスレッドが詰まって手順がまとめて走ったとき、2 本同時に起き、止めるも空振りした。2026-10-21 故障注入）
/// - 外すのは答えを受ける `finish`（メインスレッド）か、止める側。子のスレッドは外さない。外してから
///   `finish` が `session_id` を覚えるまでの間に次の送信が入ると、会話の続きが切れるため（2026-10-21 検証係の指摘）
static RUNNING: Mutex<Option<(u64, isize)>> = Mutex::new(None);
/// 会話の続きに使う `session_id`（`--resume`）。「新しいチャット」で消す。
static SESSION: Mutex<Option<String>> = Mutex::new(None);

/// チャット欄の送信。待たずに戻り、答えは後で吹き出しとチャット欄に出す。戻り値は入力欄に出す一言。
pub fn submit(app: AppHandle, text: String, auth: AuthMode) -> String {
    // 長さの上限はここでも見る（入力欄の maxlength だけでは invoke を直接呼ぶ経路を止められない）
    let Some(text) = core_chat::input_text(&text) else {
        return if text.trim().is_empty() {
            String::new()
        } else {
            format!("長すぎます（{} 字まで）", core_chat::MAX_INPUT_CHARS)
        };
    };
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
    // 開いているチャット欄を「考え中」の形にする（答えの欄と「新しいチャット」を出す）
    crate::ui::refresh_chat();
    let spawned = std::thread::Builder::new()
        .name("chat".into())
        .spawn(move || {
            let started = Instant::now();
            // panic しても finish に失敗を渡す（渡さないと予約が外れず、以後ずっと「考え中」になる。
            // Job は JobGuard が巻き戻りで閉じるので子も止まる。2026-10-21 検証係 2 巡目の指摘）
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run(generation, &text, auth, resume.as_deref())
            }))
            .unwrap_or_else(|_| {
                log("chat: worker panicked");
                failed("うまく動きませんでした")
            });
            if app
                .run_on_main_thread(move || finish(generation, outcome, started))
                .is_err()
            {
                // アプリが終わるところ。予約だけ外しておく
                release(generation);
            }
        });
    match spawned {
        Ok(_) => "考え中…".into(),
        Err(e) => {
            release(generation);
            log(format!("chat のスレッドを作れない: {e}"));
            crate::pet_window::chat_finished(BEHAVIOR_SESSION, false);
            // 予約を外したことを開いているチャット欄に伝える（伝えないと待ち中のまま送れない。3 巡目の指摘）
            crate::ui::refresh_chat();
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

/// 走っている送信を取り消し、子がいれば Job ごと止める。取り消したら true。
fn cancel_running() -> bool {
    let Ok(mut r) = RUNNING.lock() else {
        return false;
    };
    match r.take() {
        Some((generation, job)) => {
            // job が 0 なら子はまだ起きていないか、もう終わった（起こす側は予約が消えたのを見て自分で止める）
            if job != 0 {
                // job は RUNNING に入っている間は閉じられない（閉じる側は同じ鍵の中で 0 に戻してから閉じる）
                terminate_job(job);
            }
            log(format!("chat: cancelled generation {generation}"));
            true
        }
        None => false,
    }
}

/// 答えをメインスレッドで受ける。古い世代なら捨てる。`session_id` を覚えてから予約を外す。
fn finish(generation: u64, outcome: ChatOutcome, started: Instant) {
    let secs = started.elapsed().as_secs_f32();
    if GENERATION.load(Ordering::SeqCst) != generation {
        release(generation);
        log(format!("chat: discarded stale answer ({secs:.1}s)"));
        return;
    }
    // 失敗でも session_id が返れば会話は続けられる（上限・鍵違いでも id は返る。evidence E2・E11）
    let (session_id, ok) = match &outcome {
        ChatOutcome::Answer { session_id, .. } => (session_id.clone(), true),
        ChatOutcome::Failed { session_id, .. } => (session_id.clone(), false),
    };
    if let (Some(id), Ok(mut s)) = (session_id, SESSION.lock()) {
        *s = Some(id);
    }
    release(generation);
    crate::pet_window::chat_finished(BEHAVIOR_SESSION, ok);
    match outcome {
        ChatOutcome::Answer { text, .. } => {
            log(format!(
                "chat answer in {secs:.1}s ({} 字)",
                text.chars().count()
            ));
            crate::ui::show_chat_answer(&text);
        }
        ChatOutcome::Failed { reason, .. } => {
            log(format!("chat failed in {secs:.1}s"));
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

/// チャットの子プロセスを起こして答えを待つ（チャットのスレッドで走る）。
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
    let ours = move || {
        RUNNING
            .lock()
            .map(|r| r.map(|(g, _)| g) == Some(generation))
            .unwrap_or(false)
    };
    let register = move |job: isize| match RUNNING.lock() {
        Ok(mut r) if r.map(|(g, _)| g) == Some(generation) => {
            *r = Some((generation, job));
            true
        }
        _ => false,
    };
    // 予約は残し、ハンドルだけ外す（予約は finish が外す）
    let unregister = move || {
        if let Ok(mut r) = RUNNING.lock()
            && r.map(|(g, _)| g) == Some(generation)
        {
            *r = Some((generation, 0));
        }
    };
    let started = Instant::now();
    let result = run_child(ChildRun {
        tag: "chat",
        start_failed: START_FAILED,
        exe,
        args: core_chat::child_args(&prompt_file.to_string_lossy(), resume),
        env,
        cwd: work,
        input: Some(text.as_bytes().to_vec()),
        timeout: timeout(),
        register: &register,
        unregister: &unregister,
        wanted: &|| {
            // 試験用: 子が動いている最中（起こしてから PET_TEST_CHAT_PANIC ミリ秒後）にこのスレッドを
            // panic させ、巻き戻りで子が止まり、予約が外れることを確かめる（catch_unwind と JobGuard の故障注入）
            let panic_after = test_overrides()
                .then(|| std::env::var("PET_TEST_CHAT_PANIC").ok())
                .flatten()
                .and_then(|ms| ms.parse::<u64>().ok());
            if panic_after.is_some_and(|ms| started.elapsed() >= Duration::from_millis(ms)) {
                panic!("PET_TEST_CHAT_PANIC");
            }
            ours()
        },
    });
    match result {
        Ok(ChildOutput {
            code,
            stdout,
            stderr,
        }) => {
            let outcome = core_chat::parse_output(code, &stdout, &stderr);
            log(format!(
                "chat: exit={} is_error={}",
                code.map_or("none".into(), |c| c.to_string()),
                matches!(outcome, ChatOutcome::Failed { .. })
            ));
            outcome
        }
        Err(Stop::Cancelled) => failed("止めました"),
        Err(Stop::TimedOut) => {
            log(format!(
                "chat: timed out after {}s",
                timeout().as_secs_f32()
            ));
            failed(&format!("時間切れ（{} 秒）", timeout().as_secs()))
        }
        Err(Stop::Failed(reason)) => failed(reason),
    }
}

/// 自分の世代の予約が残っていれば外す（何度呼んでもよい）。
fn release(generation: u64) {
    if let Ok(mut r) = RUNNING.lock()
        && r.map(|(g, _)| g) == Some(generation)
    {
        *r = None;
    }
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

/// 設定画面の「いまの接続を確かめる」: チャットと同じ環境で `claude auth status` を走らせる（別スレッドから呼ぶ）。
/// チャットと同じく Job に入れ、時間切れと出力の上限を付ける（止まらない CLI を残さない。2026-10-21 検証係の指摘）。
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
    let work = data_dir().join("chat").join("work");
    if std::fs::create_dir_all(&work).is_err() {
        return "確かめられませんでした".into();
    }
    let result = run_child(ChildRun {
        tag: "chat",
        start_failed: START_FAILED,
        exe,
        args: vec!["auth".into(), "status".into()],
        env,
        cwd: work,
        input: None,
        timeout: AUTH_STATUS_TIMEOUT,
        register: &|_| true,
        unregister: &|| {},
        wanted: &|| true,
    });
    match result {
        Ok(out) => core_chat::auth_status_label(&out.stdout),
        Err(Stop::TimedOut) => "時間切れで確かめられませんでした".into(),
        Err(_) => "確かめられませんでした".into(),
    }
}
