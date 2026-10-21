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
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
};

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
/// 「確かめる」（`claude auth status`）の時間切れ。普段は 1 秒ほどで返る。
const AUTH_STATUS_TIMEOUT: Duration = Duration::from_secs(15);
/// 行動ロジックに渡すセッション名（hooks の出来事と混ざらない固定の名前）。
const BEHAVIOR_SESSION: &str = "pet-chat";

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
                // SAFETY: job は RUNNING に入っている間は閉じられない（閉じる側は同じ鍵の中で 0 に戻してから閉じる）
                let _ = unsafe { TerminateJobObject(HANDLE(job as *mut _), 1) };
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

/// 子プロセスを止めた・起こせなかった理由。
enum Stop {
    /// 新しいチャット・終了で取り消された
    Cancelled,
    TimedOut,
    /// 起こせなかった（理由は画面に出す短い文）
    Failed(&'static str),
}

/// 終わった子プロセスの結果。
struct ChildOutput {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// 子プロセスの起こし方と、走っている間の見張り方。
struct ChildRun<'a> {
    exe: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
    cwd: PathBuf,
    /// 標準入力に書く本文（`None` なら標準入力は空）
    input: Option<Vec<u8>>,
    timeout: Duration,
    /// Job のハンドルを止める側に知らせる。false なら（もう取り消されていた）起こさずに止める
    register: &'a dyn Fn(isize) -> bool,
    /// ハンドルを閉じる直前に呼ぶ（止める側がもう触らないように）
    unregister: &'a dyn Fn(),
    /// まだ答えを待つか（false になったら止める）
    wanted: &'a dyn Fn() -> bool,
}

/// 送信ごとの Job Object。落とすと（どの経路で抜けても、panic で巻き戻っても）止める側に知らせてから
/// ハンドルを閉じる。閉じると中の子と孫も止まる（KILL_ON_JOB_CLOSE）。
/// 手で閉じる呼び出しを経路ごとに書くと、書き漏らした経路で予約とハンドルが残る（2026-10-21 検証係の指摘）。
struct JobGuard<'a> {
    handle: HANDLE,
    unregister: &'a dyn Fn(),
}

impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        (self.unregister)();
        // SAFETY: この値が作ったハンドルを、止める側が触らなくなってから 1 回だけ閉じる
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

impl JobGuard<'_> {
    /// 中の子と孫を全部止める。
    fn terminate(&self) {
        // SAFETY: この値が持つ有効なハンドル
        let _ = unsafe { TerminateJobObject(self.handle, 1) };
    }
}

/// 子プロセスを 1 つ起こし、終わるまで待って終了コードと出力を返す。
///
/// 子は**止めた状態で**起こし（`CREATE_SUSPENDED`）、Job Object に入れてから動かす。動いてから入れると、
/// 入れるまでの間に起こした孫が Job の外に逃げる（2026-10-21 検証係の指摘）。入れられなければ動かさずに止める。
/// 止めるときは Job ごと（孫まで）止める。標準入力は別スレッドで書く（子が読まずにパイプが詰まっても、
/// 時間切れの見張りが止まらないように。同じ指摘）。補助のスレッドを作れなければ子を止めて失敗にする
/// （`thread::spawn` は失敗すると panic する。2 巡目の指摘）。
fn run_child(r: ChildRun) -> Result<ChildOutput, Stop> {
    const START_FAILED: Stop = Stop::Failed("Claude Code を起こせませんでした");
    let job = JobGuard {
        handle: new_kill_on_close_job().map_err(|e| {
            log(format!("chat: Job Object を作れない: {e}"));
            START_FAILED
        })?,
        unregister: r.unregister,
    };
    let mut cmd = Command::new(&r.exe);
    cmd.args(&r.args)
        .env_clear()
        .envs(r.env.iter().map(|(k, v)| (k, v)))
        .current_dir(&r.cwd)
        .stdin(if r.input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags((CREATE_NO_WINDOW | CREATE_SUSPENDED).0);
    let mut child = cmd.spawn().map_err(|e| {
        log(format!("chat: 起動できない: {e}"));
        START_FAILED
    })?;
    // 止めた状態のまま、Job に入れて止める側に知らせる。どれかに失敗したら動かさずに止める
    // SAFETY: child のハンドルは child が生きている間有効
    let assigned =
        unsafe { AssignProcessToJobObject(job.handle, HANDLE(child.as_raw_handle() as *mut _)) };
    if let Err(e) = assigned {
        log(format!(
            "chat: Job Object に入れられない（起こさずに止める）: {e}"
        ));
        let _ = child.kill();
        let _ = child.wait();
        return Err(START_FAILED);
    }
    // ここから先は子が Job の中にいるので、どの経路で抜けても job が落ちる時に止まる
    let stop_child = |child: &mut std::process::Child| {
        job.terminate();
        let _ = child.wait();
    };
    if !(r.register)(job.handle.0 as isize) {
        stop_child(&mut child);
        return Err(Stop::Cancelled);
    }
    if let Err(e) = resume_process(child.id()) {
        log(format!("chat: 子を動かせない: {e}"));
        stop_child(&mut child);
        return Err(START_FAILED);
    }

    // 本文は標準入力で渡す（コマンドラインに載せない＝引用符の罠とプロセス一覧からの露出を避ける）
    if let (Some(mut stdin), Some(input)) = (child.stdin.take(), r.input) {
        let writer = std::thread::Builder::new().spawn(move || {
            // 子が終わる・止められるとパイプが切れて失敗で抜ける
            let _ = stdin.write_all(&input);
        });
        if let Err(e) = writer {
            log(format!("chat: 書き込みのスレッドを作れない: {e}"));
            stop_child(&mut child);
            return Err(START_FAILED);
        }
    }
    let (out, err) = match (
        child.stdout.take().map(read_capped).transpose(),
        child.stderr.take().map(read_capped).transpose(),
    ) {
        (Ok(out), Ok(err)) => (out, err),
        _ => {
            log("chat: 読み取りのスレッドを作れない");
            stop_child(&mut child);
            return Err(START_FAILED);
        }
    };

    let deadline = Instant::now() + r.timeout;
    let stop = loop {
        match child.try_wait() {
            Ok(Some(s)) => {
                drop(job);
                let stdout = out.and_then(|h| h.join().ok()).unwrap_or_default();
                let stderr = err.and_then(|h| h.join().ok()).unwrap_or_default();
                return Ok(ChildOutput {
                    code: s.code(),
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {}
            Err(_) => break Stop::Failed("Claude Code の終わりを確かめられませんでした"),
        }
        if !(r.wanted)() {
            break Stop::Cancelled;
        }
        if Instant::now() >= deadline {
            break Stop::TimedOut;
        }
        std::thread::sleep(POLL);
    };
    stop_child(&mut child);
    Err(stop)
}

/// 止めた状態で起こしたプロセスのスレッドを動かす（新しいプロセスのスレッドは 1 本）。
fn resume_process(pid: u32) -> windows::core::Result<()> {
    // SAFETY: スナップショットのハンドルはこの関数の中で閉じる。構造体の大きさを先に入れる（API の約束）
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0)?;
        let mut entry = THREADENTRY32 {
            dwSize: size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut resumed = 0;
        let mut more = Thread32First(snap, &mut entry).is_ok();
        while more {
            if entry.th32OwnerProcessID == pid
                && let Ok(t) = OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID)
            {
                if ResumeThread(t) != u32::MAX {
                    resumed += 1;
                }
                let _ = CloseHandle(t);
            }
            more = Thread32Next(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
        if resumed == 0 {
            return Err(windows::core::Error::from_thread());
        }
        Ok(())
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
/// スレッドを作れなければ Err（panic させない）。
fn read_capped(r: impl Read + Send + 'static) -> std::io::Result<std::thread::JoinHandle<Vec<u8>>> {
    std::thread::Builder::new().spawn(move || {
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
