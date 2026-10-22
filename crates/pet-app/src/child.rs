//! 子プロセスを 1 つ起こして終わりを待つ（チャットの `claude -p` と、音声入力の文字起こしで共有する）。
//!
//! チャット（Phase 3 前半）で検証係の指摘を受けて固めた起こし方を、音声入力（Phase 3 後半）でも
//! 同じに使うために `chat.rs` から移した（振る舞いは変えていない。設計 `.steering/20261022-phase3-voice/design.md` §3）。
//!
//! - 子は止めた状態で起こして Job Object（閉じると中身ごと止まる）に入れてから動かす
//! - 標準入力は別スレッドで書き、出力は上限まで別スレッドで読む
//! - 時間切れ・取り消しは Job ごと（孫まで）止める

use std::io::{Read, Write};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

use crate::log::log;

/// 標準出力・標準エラーを読む上限（暴走した出力で pet-app のメモリを食わないように。答えは数 KB）。
const MAX_OUTPUT_BYTES: u64 = 1 << 20;
/// 子プロセスの終わりを見る間隔。
const POLL: Duration = Duration::from_millis(100);

/// 子プロセスを止めた・起こせなかった理由。
pub enum Stop {
    /// 新しいチャット・終了で取り消された
    Cancelled,
    TimedOut,
    /// 起こせなかった（理由は画面に出す短い文）
    Failed(&'static str),
}

/// 終わった子プロセスの結果。
pub struct ChildOutput {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// 子プロセスの起こし方と、走っている間の見張り方。
pub struct ChildRun<'a> {
    /// ログの頭に付ける名前（`chat`・`stt`）
    pub tag: &'static str,
    /// 起こせなかった時に画面に出す短い文
    pub start_failed: &'static str,
    pub exe: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    /// 標準入力に書く本文（`None` なら標準入力は空）
    pub input: Option<Vec<u8>>,
    pub timeout: Duration,
    /// Job のハンドルを止める側に知らせる。false なら（もう取り消されていた）起こさずに止める
    pub register: &'a dyn Fn(isize) -> bool,
    /// ハンドルを閉じる直前に呼ぶ（止める側がもう触らないように）
    pub unregister: &'a dyn Fn(),
    /// まだ答えを待つか（false になったら止める）
    pub wanted: &'a dyn Fn() -> bool,
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
pub fn run_child(r: ChildRun) -> Result<ChildOutput, Stop> {
    let tag = r.tag;
    let start_failed = Stop::Failed(r.start_failed);
    let job = JobGuard {
        handle: match new_kill_on_close_job() {
            Ok(h) => h,
            Err(e) => {
                log(format!("{tag}: Job Object を作れない: {e}"));
                return Err(start_failed);
            }
        },
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
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            log(format!("{tag}: 起動できない: {e}"));
            return Err(start_failed);
        }
    };
    // 止めた状態のまま、Job に入れて止める側に知らせる。どれかに失敗したら動かさずに止める
    // SAFETY: child のハンドルは child が生きている間有効
    let assigned =
        unsafe { AssignProcessToJobObject(job.handle, HANDLE(child.as_raw_handle() as *mut _)) };
    if let Err(e) = assigned {
        log(format!(
            "{tag}: Job Object に入れられない（起こさずに止める）: {e}"
        ));
        let _ = child.kill();
        let _ = child.wait();
        return Err(start_failed);
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
        log(format!("{tag}: 子を動かせない: {e}"));
        stop_child(&mut child);
        return Err(start_failed);
    }

    // 本文は標準入力で渡す（コマンドラインに載せない＝引用符の罠とプロセス一覧からの露出を避ける）
    if let (Some(mut stdin), Some(input)) = (child.stdin.take(), r.input) {
        let writer = std::thread::Builder::new().spawn(move || {
            // 子が終わる・止められるとパイプが切れて失敗で抜ける
            let _ = stdin.write_all(&input);
        });
        if let Err(e) = writer {
            log(format!("{tag}: 書き込みのスレッドを作れない: {e}"));
            stop_child(&mut child);
            return Err(start_failed);
        }
    }
    let (out, err) = match (
        child.stdout.take().map(read_capped).transpose(),
        child.stderr.take().map(read_capped).transpose(),
    ) {
        (Ok(out), Ok(err)) => (out, err),
        _ => {
            log(format!("{tag}: 読み取りのスレッドを作れない"));
            stop_child(&mut child);
            return Err(start_failed);
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
            Err(_) => break Stop::Failed("子プロセスの終わりを確かめられませんでした"),
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

/// 閉じると中のプロセスが全部止まる Job Object。
pub fn new_kill_on_close_job() -> windows::core::Result<HANDLE> {
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

/// Job ごと（孫まで）止める。`handle` は止める側が預かっている間、閉じられないこと。
pub fn terminate_job(handle: isize) {
    // SAFETY: 呼び出し側が、ハンドルが閉じられていないことを鍵で守っている（chat.rs の RUNNING と同じ約束）
    let _ = unsafe { TerminateJobObject(HANDLE(handle as *mut _), 1) };
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
pub fn test_overrides() -> bool {
    cfg!(debug_assertions) && std::env::var_os("PET_CLAUDE_DATA_DIR").is_some()
}
