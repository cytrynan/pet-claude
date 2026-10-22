//! 読み上げ（Phase 3 後半。設計 `.steering/20261022-phase3-voice/design.md` §2）。
//!
//! AivisSpeech（VOICEVOX 互換の HTTP。`127.0.0.1:10101`）で天深シノ・ノーマルの声を作り、
//! `PlaySoundW` で鳴らす。専用のスレッド 1 本が `pet_core::voice::SpeechQueue` に従って 1 つずつ話す。
//!
//! - 呼ぶ側（吹き出しを出す所）は文を投げるだけで待たない
//! - エンジンが動いていなければ、ペットが起こす（Job Object に入れる＝pet-app が落ちても残らない）。
//!   ペットが起こした時だけ、最後に話してから `ENGINE_IDLE` で止める。既に動いていたエンジンには触らない
//! - 失敗（エンジンが無い・応答しない・壊れた音）は黙ってログに 1 行。アプリは止めない（V2）
//! - ログには字数と秒数だけ。文そのものは書かない（N4）

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use pet_core::voice::{self, SpeechQueue, Utterance};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Audio::{PlaySoundW, SND_MEMORY, SND_NODEFAULT, SND_SYNC};
use windows::Win32::System::JobObjects::AssignProcessToJobObject;
use windows::Win32::System::Threading::CREATE_NO_WINDOW;
use windows::core::PCWSTR;

use crate::log::log;

const ENGINE_ADDR: &str = "127.0.0.1:10101";
const SPEAKER: &str = "天深シノ";
const STYLE: &str = "ノーマル";
/// `/version` の待ち時間（動いていればすぐ返る）。
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
/// 合成の待ち時間（140 字で数秒。初回はモデルの読み込みで長い）。
const SYNTH_TIMEOUT: Duration = Duration::from_secs(30);
/// エンジンを起こしてから使えるまで待つ上限（ai-news の radio.mjs と同じ 90 秒）。
const ENGINE_START_TIMEOUT: Duration = Duration::from_secs(90);
/// 起こすのに時間がかかったら、待たせていた固定文言は話さない（数十秒後の「できたよ」は意味がずれる）。
const STALE_PHRASE: Duration = Duration::from_secs(5);
/// ペットが起こしたエンジンを止めるまでの、話さなかった時間（エンジンは約 3.4GB を使う。2026-10-22 実測）。
const ENGINE_IDLE: Duration = Duration::from_secs(600);
/// 受け取る音の上限（140 字で 1MB 前後）。
const MAX_RESPONSE_BYTES: usize = 32 << 20;

/// 「声で知らせる」。オフの間は何も受けず、話している途中でも止める。
static ENABLED: AtomicBool = AtomicBool::new(false);
static WORKER_STARTED: AtomicBool = AtomicBool::new(false);
static QUEUE: Mutex<Option<SpeechQueue>> = Mutex::new(None);
static WAKE: Condvar = Condvar::new();
/// ペットが起こしたエンジンの Job Object（閉じると止まる）。0 は起こしていない。
static ENGINE_JOB: Mutex<isize> = Mutex::new(0);

/// 話す。待たずに戻る。オフなら何もしない。
pub fn speak(u: Utterance) {
    if !ENABLED.load(Ordering::SeqCst) {
        return;
    }
    ensure_worker();
    let now = now_ms();
    if let Ok(mut q) = QUEUE.lock() {
        q.get_or_insert_with(SpeechQueue::new).push(u, now);
    }
    WAKE.notify_all();
}

/// 固定文言を話す（`pet_core::voice::PHRASE_*`）。
pub fn phrase(text: &str) {
    speak(Utterance::Phrase(text.to_string()));
}

/// チャットの答えを話す（吹き出しと同じ検査を通した 1 行。V1）。
pub fn answer(text: &str) {
    if let Some(line) = voice::speech_for_answer(text) {
        speak(Utterance::Answer(line));
    }
}

/// 設定の「声で知らせる」。オフにしたら待っている文を捨て、話している途中でも止める（V4）。
pub fn set_enabled(on: bool) {
    let was = ENABLED.swap(on, Ordering::SeqCst);
    if was && !on {
        if let Ok(mut q) = QUEUE.lock()
            && let Some(q) = q.as_mut()
        {
            q.clear();
        }
        stop_playback();
        log("tts: off");
    }
    WAKE.notify_all();
}

/// 終了: 話すのを止め、ペットが起こしたエンジンを止める。
pub fn shutdown() {
    ENABLED.store(false, Ordering::SeqCst);
    stop_playback();
    stop_engine("quit");
}

/// 設定画面に出す読み上げの状態（別スレッドから呼ぶ。最大 2 秒待つ）。
pub fn status() -> &'static str {
    if http("GET", "/version", &[], PROBE_TIMEOUT).is_ok() {
        "使えます"
    } else if engine_exe().is_some() {
        "エンジンは話す時に起こします"
    } else {
        "エンジンが見つかりません"
    }
}

fn ensure_worker() {
    if WORKER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new().name("tts".into()).spawn(worker);
    if let Err(e) = spawned {
        WORKER_STARTED.store(false, Ordering::SeqCst);
        log(format!("tts: スレッドを作れない: {e}"));
    }
}

/// 話す番が来るのを待ち、1 つずつ合成して鳴らす。ペットが起こしたエンジンは暇になったら止める。
fn worker() {
    let mut style: Option<u32> = None;
    let mut last_spoke = Instant::now();
    loop {
        let text = {
            let Ok(mut guard) = QUEUE.lock() else { return };
            loop {
                let q = guard.get_or_insert_with(SpeechQueue::new);
                if let Some(t) = q.next(now_ms()) {
                    break t;
                }
                let (g, timeout) = WAKE
                    .wait_timeout(guard, Duration::from_secs(30))
                    .unwrap_or_else(|e| e.into_inner());
                guard = g;
                if timeout.timed_out() && last_spoke.elapsed() >= ENGINE_IDLE {
                    stop_engine("idle");
                }
            }
        };
        let started = Instant::now();
        let result = speak_one(&text, &mut style, started);
        last_spoke = Instant::now();
        if let Err(why) = result {
            log(format!("tts: 話せなかった（{why}）"));
        }
        if let Ok(mut q) = QUEUE.lock()
            && let Some(q) = q.as_mut()
        {
            q.finished(now_ms());
        }
    }
}

fn speak_one(text: &str, style: &mut Option<u32>, started: Instant) -> Result<(), String> {
    ensure_engine()?;
    if is_fixed_phrase(text) && started.elapsed() > STALE_PHRASE {
        return Err("エンジンを起こすのに時間がかかったので固定文言を捨てた".into());
    }
    if !ENABLED.load(Ordering::SeqCst) {
        return Ok(());
    }
    let id = match *style {
        Some(id) => id,
        None => {
            let id = resolve_style()?;
            *style = Some(id);
            id
        }
    };
    let synth = Instant::now();
    let query = http(
        "POST",
        &format!("/audio_query?speaker={id}&text={}", percent_encode(text)),
        &[],
        SYNTH_TIMEOUT,
    )?;
    let wav = http(
        "POST",
        &format!("/synthesis?speaker={id}"),
        &query,
        SYNTH_TIMEOUT,
    )?;
    let mut wav = wav;
    if voice::parse_wav(&wav).is_none() {
        return Err("エンジンが返した音を読めない".into());
    }
    // RIFF の長さは parse_wav が見ない欄。PlaySoundW はこの欄を信じて読むので、実際の長さに書き直す
    // （中のチャンクが収まることは parse_wav が確かめた。2026-10-22 セキュリティレビューの指摘）
    let Ok(riff_len) = u32::try_from(wav.len() - 8) else {
        return Err("エンジンが返した音が大きすぎる".into());
    };
    wav[4..8].copy_from_slice(&riff_len.to_le_bytes());
    log(format!(
        "tts: {} 字を {:.1}s で合成",
        text.chars().count(),
        synth.elapsed().as_secs_f32()
    ));
    if !ENABLED.load(Ordering::SeqCst) {
        return Ok(());
    }
    // SAFETY: wav は鳴り終わる（SND_SYNC で戻る）まで生きている。止めるのは別スレッドの PlaySoundW(NULL)
    let _ = unsafe {
        PlaySoundW(
            PCWSTR(wav.as_ptr().cast()),
            None,
            SND_MEMORY | SND_SYNC | SND_NODEFAULT,
        )
    };
    Ok(())
}

fn is_fixed_phrase(text: &str) -> bool {
    [
        voice::PHRASE_DONE,
        voice::PHRASE_FAILED,
        voice::PHRASE_WAITING,
        voice::PHRASE_PERMISSION,
    ]
    .contains(&text)
}

fn stop_playback() {
    // SAFETY: NULL を渡すと、このプロセスで鳴っている音を止める（API の約束）
    let _ = unsafe { PlaySoundW(PCWSTR::null(), None, SND_NODEFAULT) };
}

/// 天深シノ・ノーマルの style id を名前で引く（先頭のスタイルに逃げない＝kuroko-voice の教訓）。
fn resolve_style() -> Result<u32, String> {
    let body = http("GET", "/speakers", &[], SYNTH_TIMEOUT)?;
    let v: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| "話者の一覧を読めない".to_string())?;
    v.as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["name"] == SPEAKER)
        .flat_map(|s| s["styles"].as_array().into_iter().flatten())
        .find(|st| st["name"] == STYLE)
        .and_then(|st| st["id"].as_u64())
        .and_then(|id| u32::try_from(id).ok())
        .ok_or_else(|| format!("{SPEAKER}・{STYLE} が見つからない"))
}

/// エンジンが応答するようにする。動いていなければ起こして待つ。
fn ensure_engine() -> Result<(), String> {
    if http("GET", "/version", &[], PROBE_TIMEOUT).is_ok() {
        return Ok(());
    }
    let Some(exe) = engine_exe() else {
        return Err("エンジンが見つからない".into());
    };
    start_engine(&exe)?;
    let deadline = Instant::now() + ENGINE_START_TIMEOUT;
    while Instant::now() < deadline {
        if !ENABLED.load(Ordering::SeqCst) {
            return Err("待っている間にオフになった".into());
        }
        std::thread::sleep(Duration::from_secs(1));
        if http("GET", "/version", &[], PROBE_TIMEOUT).is_ok() {
            log("tts: engine ready");
            return Ok(());
        }
    }
    stop_engine("start timed out");
    Err("エンジンが起きない".into())
}

/// エンジンを窓なしで起こし、閉じると止まる Job に入れる（pet-app が落ちても残さない）。
fn start_engine(exe: &std::path::Path) -> Result<(), String> {
    let Ok(mut slot) = ENGINE_JOB.lock() else {
        return Err("内部の鍵".into());
    };
    if *slot != 0 {
        // 起こしてまだ応答していないだけ（前の待ちが時間切れになる前に来た）
        return Ok(());
    }
    let job = crate::child::new_kill_on_close_job().map_err(|e| format!("Job Object: {e}"))?;
    let spawned = Command::new(exe)
        .current_dir(exe.parent().unwrap_or(exe))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW.0)
        .spawn();
    let child = match spawned {
        Ok(c) => c,
        Err(e) => {
            // SAFETY: いま作ったハンドル
            let _ = unsafe { CloseHandle(job) };
            return Err(format!("起こせない: {e}"));
        }
    };
    // SAFETY: child のハンドルは child が生きている間有効。job はいま作ったもの
    let assigned =
        unsafe { AssignProcessToJobObject(job, HANDLE(child.as_raw_handle() as *mut _)) };
    if assigned.is_err() {
        // Job に入れられないなら、残る恐れがあるので止める（エンジンは大きい）
        let mut child = child;
        let _ = child.kill();
        // SAFETY: いま作ったハンドル
        let _ = unsafe { CloseHandle(job) };
        return Err("エンジンを Job に入れられない".into());
    }
    *slot = job.0 as isize;
    log("tts: engine started");
    Ok(())
}

/// ペットが起こしたエンジンだけ止める（Job を閉じると中身ごと止まる）。
fn stop_engine(why: &str) {
    if let Ok(mut slot) = ENGINE_JOB.lock()
        && *slot != 0
    {
        // SAFETY: start_engine が作り、この鍵の中でしか触らないハンドル
        let _ = unsafe { CloseHandle(HANDLE(*slot as *mut _)) };
        *slot = 0;
        log(format!("tts: engine stopped ({why})"));
    }
}

/// エンジンの exe（ai-news と同じ置き場所。試験では `PET_TEST_AIVIS_EXE` で差し替え・空なら無い扱い）。
fn engine_exe() -> Option<PathBuf> {
    if crate::child::test_overrides()
        && let Some(p) = std::env::var_os("PET_TEST_AIVIS_EXE")
    {
        return (!p.is_empty()).then(|| PathBuf::from(p));
    }
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from)?;
    let exe = home.join(r"dev\voice\_engines\AivisSpeech\AivisSpeech-Engine\run.exe");
    exe.is_file().then_some(exe)
}

/// エンジンの宛先（試験では `PET_TEST_AIVIS_ADDR` で偽のエンジンへ）。
fn engine_addr() -> SocketAddr {
    let fallback: SocketAddr = ENGINE_ADDR.parse().expect("固定の宛先");
    if crate::child::test_overrides()
        && let Ok(a) = std::env::var("PET_TEST_AIVIS_ADDR")
    {
        return a.parse().unwrap_or(fallback);
    }
    fallback
}

/// 127.0.0.1 のエンジンへ 1 回だけ HTTP/1.1 で問い合わせる（新しい crate を足さないため手で書く）。
/// 2xx の本文を返す。それ以外・時間切れ・壊れた応答は Err。
fn http(method: &str, path: &str, body: &[u8], timeout: Duration) -> Result<Vec<u8>, String> {
    let addr = engine_addr();
    let mut s = TcpStream::connect_timeout(&addr, PROBE_TIMEOUT.min(timeout))
        .map_err(|_| "エンジンにつながらない".to_string())?;
    let _ = s.set_read_timeout(Some(timeout));
    let _ = s.set_write_timeout(Some(timeout));
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes())
        .and_then(|_| s.write_all(body))
        .map_err(|_| "送れない".to_string())?;
    let mut buf = Vec::new();
    s.take(MAX_RESPONSE_BYTES as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|_| "応答が来ない".to_string())?;
    if buf.len() > MAX_RESPONSE_BYTES {
        return Err("応答が大きすぎる".into());
    }
    match parse_response(&buf) {
        Some((code, body)) if (200..300).contains(&code) => Ok(body),
        Some((code, _)) => Err(format!("HTTP {code}")),
        None => Err("応答を読めない".into()),
    }
}

/// HTTP/1.1 の応答を状態コードと本文に分ける（`Content-Length` か chunked。どちらも無ければ残り全部）。
fn parse_response(buf: &[u8]) -> Option<(u16, Vec<u8>)> {
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&buf[..split]).ok()?;
    let rest = &buf[split + 4..];
    let mut lines = head.split("\r\n");
    let status = lines.next()?;
    let mut parts = status.splitn(3, ' ');
    if !parts.next()?.starts_with("HTTP/1.") {
        return None;
    }
    let code: u16 = parts.next()?.parse().ok()?;
    let mut length = None;
    let mut chunked = false;
    for line in lines {
        let (k, v) = line.split_once(':')?;
        let (k, v) = (k.trim(), v.trim());
        if k.eq_ignore_ascii_case("content-length") {
            length = Some(v.parse::<usize>().ok()?);
        } else if k.eq_ignore_ascii_case("transfer-encoding") && v.eq_ignore_ascii_case("chunked") {
            chunked = true;
        }
    }
    let body = if chunked {
        dechunk(rest)?
    } else if let Some(n) = length {
        rest.get(..n)?.to_vec()
    } else {
        rest.to_vec()
    };
    Some((code, body))
}

/// chunked の本文をつなぐ。最後の 0 の塊まで揃っていなければ None（途中で切れた音を鳴らさない）。
fn dechunk(mut rest: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let eol = rest.windows(2).position(|w| w == b"\r\n")?;
        let size_line = std::str::from_utf8(&rest[..eol]).ok()?;
        let size = usize::from_str_radix(size_line.split(';').next()?.trim(), 16).ok()?;
        rest = &rest[eol + 2..];
        if size == 0 {
            return Some(out);
        }
        out.extend_from_slice(rest.get(..size)?);
        rest = rest.get(size..)?.strip_prefix(b"\r\n")?;
    }
}

/// クエリ文字列に入れるための UTF-8 のパーセント符号化（英数字と `-_.~` 以外）。
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// 話す順の決まり（SpeechQueue）に渡す単調増加の時刻。
fn now_ms() -> u64 {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_length() {
        let r = b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nX: y\r\n\r\nabcdef";
        assert_eq!(parse_response(r), Some((200, b"abc".to_vec())));
    }

    #[test]
    fn short_body_is_rejected() {
        let r = b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nabc";
        assert_eq!(parse_response(r), None);
    }

    #[test]
    fn parses_chunked() {
        let r = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2;x=1\r\nde\r\n0\r\n\r\n";
        assert_eq!(parse_response(r), Some((200, b"abcde".to_vec())));
    }

    #[test]
    fn truncated_chunked_is_rejected() {
        let r = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n5\r\nde";
        assert_eq!(parse_response(r), None);
    }

    #[test]
    fn keeps_status_code() {
        let r = b"HTTP/1.1 422 Unprocessable Entity\r\nContent-Length: 0\r\n\r\n";
        assert_eq!(parse_response(r), Some((422, vec![])));
    }

    #[test]
    fn rejects_non_http() {
        assert_eq!(parse_response(b"garbage\r\n\r\n"), None);
        assert_eq!(parse_response(b"HTTP/1.1 200 OK"), None);
    }

    #[test]
    fn encodes_japanese_and_reserved() {
        assert_eq!(percent_encode("a b&c"), "a%20b%26c");
        assert_eq!(percent_encode("あ"), "%E3%81%82");
        assert_eq!(percent_encode("A-z_0.~"), "A-z_0.~");
    }
}
