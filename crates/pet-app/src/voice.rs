//! バーの音声入力（Phase 3 後半。設計 `.steering/20261022-phase3-voice/design.md` §3）。
//!
//! ボタンで録音を始め、話し終えて黙るか、もう一度押すと止める。録音を WAV にして文字起こしの子
//! （faster-whisper の small）に渡し、結果をチャットに送る（S11）。
//!
//! - 録音は winmm の `waveIn*` で 16kHz・モノラル・16bit を直接受ける（区切りは `pet_core::voice::Endpointer`）
//! - 文字起こしの子はチャットと同じ起こし方（`child.rs`。Job Object・時間切れ・取り消し）
//! - 録音した WAV は文字起こしが終わったら成否に関係なく消す（V9）
//! - 同時に 1 本だけ。世代番号で、取り消した後の古い結果を捨てる
//! - ログには秒数と字数だけ。文そのものは書かない（N4）

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use pet_core::voice::{self, EndReason, EndpointState, Endpointer};
use tauri::AppHandle;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    CALLBACK_NULL, HWAVEIN, WAVE_FORMAT_PCM, WAVE_MAPPER, WAVEFORMATEX, WAVEHDR, WHDR_DONE,
    waveInAddBuffer, waveInClose, waveInGetNumDevs, waveInOpen, waveInPrepareHeader, waveInReset,
    waveInStart, waveInUnprepareHeader,
};
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eConsole};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::core::PSTR;

use crate::child::{ChildRun, Input, Stop, run_child, terminate_job, test_overrides};
use crate::log::{data_dir, log};

/// 文字起こしの時間切れ。子は録音を始めた時に起こすので、録音（最長 15 秒）とモデルの読み込みも含む
/// （6 秒の発話で読み込み 1.7 秒＋認識 1.6 秒。初回はディスクから読むので長い）。
const STT_TIMEOUT: Duration = Duration::from_secs(90);
/// 録音のバッファの数（30ms × 8 = 240ms ぶん。取りに行くのが遅れても途切れないように）。
const BUFFERS: usize = 8;
/// バッファが埋まったかを見る間隔。
const POLL: Duration = Duration::from_millis(10);

/// 世代番号。始めるたび・取り消しのたびに進める（古い結果を捨てる印）。
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// 動いている音声入力（世代, 文字起こしの Job のハンドル。子を起こす前と後は 0）。
static RUNNING: Mutex<Option<(u64, isize)>> = Mutex::new(None);
/// もう一度押された（録音を止めて送る）。
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
/// 録音中か（バーのボタンの見た目）。
static RECORDING: AtomicBool = AtomicBool::new(false);

/// 音声入力の結果。
enum Outcome {
    Text(String),
    /// 吹き出しに出す短い文（`pet_core::voice::PHRASE_NOT_HEARD` など）
    Failed(&'static str),
    Cancelled,
}

/// バーの音声ボタン（メインスレッド）。録音中ならもう一度押した＝止めて送る。
pub fn toggle(app: AppHandle) {
    let Ok(mut r) = RUNNING.lock() else { return };
    if r.is_some() {
        if RECORDING.load(Ordering::SeqCst) {
            STOP_REQUESTED.store(true, Ordering::SeqCst);
            log("voice: stop requested");
        }
        return;
    }
    if crate::chat::busy() {
        drop(r);
        crate::ui::show_status("まだ考え中だよ");
        return;
    }
    // 文字起こしの道具が無いのに録音させない（話し終えてから「準備が要る」と言われないように）
    if stt_python().is_none() || !stt_script().is_file() {
        drop(r);
        crate::ui::show_status("音声入力の準備が要るよ（設定を見てね）");
        log("voice: failed: 音声入力の準備が要るよ（設定を見てね）");
        return;
    }
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    *r = Some((generation, 0));
    drop(r);
    STOP_REQUESTED.store(false, Ordering::SeqCst);
    RECORDING.store(true, Ordering::SeqCst);
    crate::pet_window::chat_listening(crate::chat::BEHAVIOR_SESSION);
    crate::ui::refresh_bar();
    log("voice: start");
    let spawned = std::thread::Builder::new()
        .name("voice".into())
        .spawn(move || {
            let run_app = app.clone();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run(&run_app, generation)
            }))
            .unwrap_or_else(|_| {
                log("voice: worker panicked");
                Outcome::Failed("うまく動かなかったよ")
            });
            let handle = app.clone();
            if app
                .run_on_main_thread(move || finish(handle, generation, outcome))
                .is_err()
            {
                release(generation);
            }
        });
    if let Err(e) = spawned {
        log(format!("voice: スレッドを作れない: {e}"));
        release(generation);
        RECORDING.store(false, Ordering::SeqCst);
        crate::pet_window::chat_finished(crate::chat::BEHAVIOR_SESSION, false);
        crate::ui::refresh_bar();
    }
}

/// 録音中か（バーの見た目に使う）。
pub fn recording() -> bool {
    RECORDING.load(Ordering::SeqCst)
}

/// 終了・取り消し: 録音を止め、文字起こしの子がいれば Job ごと止める。
pub fn shutdown() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    STOP_REQUESTED.store(true, Ordering::SeqCst);
    if let Ok(mut r) = RUNNING.lock()
        && let Some((_, job)) = r.take()
        && job != 0
    {
        terminate_job(job);
    }
}

/// 起動時: 前回の録音が残っていれば消す（落ちた時に消し損ねた分。V9）。
pub fn cleanup_leftovers() {
    let dir = data_dir().join("voice");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "wav") {
            let _ = std::fs::remove_file(&p);
        }
    }
}

/// 設定画面に出す音声入力の状態。
pub fn status() -> &'static str {
    if stt_python().is_none() || !stt_script().is_file() {
        "準備が要ります（scripts\\setup-stt.ps1）"
    } else if !test_overrides() && unsafe { waveInGetNumDevs() } == 0 {
        // SAFETY: 引数なしの問い合わせ
        "マイクが見つかりません"
    } else if !test_overrides() && default_mic_muted() == Some(true) {
        "マイクがミュートになっています（Windows のサウンドの設定）"
    } else {
        "使えます"
    }
}

/// 結果をメインスレッドで受ける。古い世代なら捨てる。
fn finish(app: AppHandle, generation: u64, outcome: Outcome) {
    RECORDING.store(false, Ordering::SeqCst);
    let current = GENERATION.load(Ordering::SeqCst) == generation;
    release(generation);
    crate::ui::refresh_bar();
    if !current {
        log("voice: discarded stale result");
        return;
    }
    match outcome {
        Outcome::Text(text) => {
            log(format!("voice: {} 字をチャットへ", text.chars().count()));
            crate::ui::show_voice_text(&text);
            let reply = crate::chat::submit(app, text, crate::pet_window::chat_setup());
            if reply != "考え中…" {
                // 送れなかった（打ったチャットの返事を待っている・長すぎる）。聞き取った文は「送った」ように
                // 見えないよう消し、吹き出しで知らせる（2026-10-24 検証係の指摘: 黙って送らず、文だけ出ていた）
                crate::ui::clear_voice_text();
                crate::ui::refresh_chat();
                // 打ったチャットがまだ考え中なら、その「考え中」の動きを失敗で終わらせない（同じセッション名）
                if !crate::chat::busy() {
                    crate::pet_window::chat_finished(crate::chat::BEHAVIOR_SESSION, false);
                }
                crate::ui::show_status("話した文を送れなかったよ");
                crate::tts::phrase("話した文を送れなかったよ");
                log("voice: chat refused the transcript; told the user");
            }
        }
        Outcome::Failed(why) => {
            // why は固定の短い文（録音・文字起こしの中身は含まない）
            log(format!("voice: failed: {why}"));
            crate::pet_window::chat_finished(crate::chat::BEHAVIOR_SESSION, false);
            crate::ui::show_status(why);
            crate::tts::phrase(why);
        }
        Outcome::Cancelled => {
            log("voice: cancelled");
            crate::pet_window::chat_finished(crate::chat::BEHAVIOR_SESSION, true);
        }
    }
}

/// この世代の音声入力がまだ生きているか（取り消し・終了で false）。
fn ours_for(generation: u64) -> impl Fn() -> bool + Send + 'static {
    move || {
        RUNNING
            .lock()
            .map(|r| r.map(|(g, _)| g) == Some(generation))
            .unwrap_or(false)
    }
}

/// 録音 → WAV → 文字起こし（音声入力のスレッドで走る）。
///
/// 文字起こしの子は録音を始める前に起こし、K が話している間に Python の起動とモデルの読み込みを済ませる
/// （2026-10-22 の実測で、話し終えてから文字起こしが返るまで 6.3〜9.8 秒のうち 3〜5 秒がこの準備だった）。
/// WAV のパスは録り終えてから標準入力で渡す。録音が取りやめになったら渡さずに閉じる（子は何もせず終わる）。
/// AivisSpeech も同じ時に起こしておく（初回の 20 秒待ちを、話して考えている間に隠す）。
fn run(app: &AppHandle, generation: u64) -> Outcome {
    let ours = ours_for(generation);
    let started = Instant::now();
    let dir = data_dir().join("voice");
    if std::fs::create_dir_all(&dir).is_err() {
        return Outcome::Failed("録音を置けなかったよ");
    }
    let (send_path, path_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let stt = {
        let dir = dir.clone();
        std::thread::Builder::new()
            .name("stt".into())
            .spawn(move || transcribe(generation, path_rx, &dir))
    };
    let Ok(stt) = stt else {
        return Outcome::Failed("文字起こしを起こせなかったよ");
    };
    crate::tts::prewarm();
    let samples = match record(&ours) {
        Ok(s) => s,
        Err(o) => {
            // 子は標準入力が閉じて終わる。結果は待たない（取りやめを早く伝える）
            drop(send_path);
            return o;
        }
    };
    RECORDING.store(false, Ordering::SeqCst);
    // 録り終えたらバーのボタンを戻す（文字起こしの間も赤いまま「止めて送る」と出ていた。2026-10-24 検証係の指摘）
    let _ = app.run_on_main_thread(crate::ui::refresh_bar);
    log(format!(
        "voice: recorded {:.1}s",
        samples.len() as f32 / voice::SAMPLE_RATE as f32
    ));
    if !ours() {
        return Outcome::Cancelled;
    }
    crate::pet_window::chat_thinking(crate::chat::BEHAVIOR_SESSION);
    let wav = dir.join(format!("rec-{generation}.wav"));
    if std::fs::write(&wav, voice::wav_bytes(&samples, voice::SAMPLE_RATE)).is_err() {
        let _ = std::fs::remove_file(&wav);
        return Outcome::Failed("録音を置けなかったよ");
    }
    let mut line = wav.to_string_lossy().into_owned().into_bytes();
    line.push(b'\n');
    let _ = send_path.send(line);
    let outcome = stt.join().unwrap_or_else(|_| {
        log("stt: thread panicked");
        Outcome::Failed("うまく動かなかったよ")
    });
    // 成否に関係なく消す（V9）
    let _ = std::fs::remove_file(&wav);
    log(format!(
        "voice: done in {:.1}s",
        started.elapsed().as_secs_f32()
    ));
    outcome
}

/// マイクから録る。区切りは `Endpointer`、もう一度押されたら止める。
/// 試験では `PET_TEST_VOICE_WAV` の WAV をマイクの代わりに 30ms ずつ流し込む。
fn record(ours: &dyn Fn() -> bool) -> Result<Vec<i16>, Outcome> {
    let mut ep = Endpointer::new();
    let mut all = Vec::new();
    let mut on_frame = |frame: &[i16], all: &mut Vec<i16>| -> Option<EndReason> {
        all.extend_from_slice(frame);
        match ep.push(frame) {
            EndpointState::Ended(r) => Some(r),
            EndpointState::Listening => None,
        }
    };
    let reason = if let Some(path) = test_wav() {
        let samples = read_test_wav(&path).ok_or(Outcome::Failed("録音を読めなかったよ"))?;
        let mut reason = None;
        for frame in samples.chunks(voice::FRAME_SAMPLES) {
            if !ours() {
                return Err(Outcome::Cancelled);
            }
            if STOP_REQUESTED.load(Ordering::SeqCst) {
                break;
            }
            reason = on_frame(frame, &mut all);
            if reason.is_some() {
                break;
            }
            // 本物のマイクと同じ速さで流す（取り消し・もう一度押すの試験のため）
            std::thread::sleep(Duration::from_millis(30));
        }
        // 流し終えたら止めたのと同じ（ファイルの終わり）
        reason
    } else {
        record_mic(ours, &mut |f, a| on_frame(f, a), &mut all)?
    };
    // None は押して止めた（か、試験の WAV が終わった）。音量は数字だけ（聞き取れない時の切り分け用。N4）
    let peak = all.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
    log(format!(
        "voice: end {reason:?} peak={peak} heard={}",
        ep.heard_speech()
    ));
    // ほぼ無音は、ミュートや Windows のプライバシー設定でマイクが止められている時の形
    // （2026-10-22 K の PC: ミュートで最大振幅 1。完全な 0 ではなかったので 0 だけを見る判定では漏れた）
    if !ep.heard_speech() && voice::looks_muted(&all) {
        return Err(Outcome::Failed(
            "マイクを使えないよ（ミュートか Windows の設定）",
        ));
    }
    // 話し終えた・押して止めた・最長になった・5 秒黙っていた: 話していれば送る
    if ep.heard_speech() {
        Ok(all)
    } else {
        Err(Outcome::Failed(voice::PHRASE_NOT_HEARD))
    }
}

type FrameSink<'a> = dyn FnMut(&[i16], &mut Vec<i16>) -> Option<EndReason> + 'a;

/// winmm でマイクを開いて録る。戻り値は区切りの理由（押して止めたら None）。
fn record_mic(
    ours: &dyn Fn() -> bool,
    on_frame: &mut FrameSink,
    all: &mut Vec<i16>,
) -> Result<Option<EndReason>, Outcome> {
    // SAFETY: 引数なしの問い合わせ
    if unsafe { waveInGetNumDevs() } == 0 {
        return Err(Outcome::Failed("マイクが見つからないよ"));
    }
    // ミュートなら録る前に言う（ミュートのマイクは無音を返し続け、5 秒待ってから「聞き取れない」になっていた）
    if default_mic_muted() == Some(true) {
        log("voice: default capture device is muted");
        return Err(Outcome::Failed("マイクがミュートになっているよ"));
    }
    let format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: 1,
        nSamplesPerSec: voice::SAMPLE_RATE,
        nAvgBytesPerSec: voice::SAMPLE_RATE * 2,
        nBlockAlign: 2,
        wBitsPerSample: 16,
        cbSize: 0,
    };
    let hdr_size = size_of::<WAVEHDR>() as u32;
    // バッファと頭はマイクより先に作る。Rust は後に作った値から落とすので、どの経路で抜けても
    // マイクの片付け（止める・頭を外す・閉じる）がバッファを捨てるより先に走る（2026-10-22 セキュリティレビューの指摘:
    // 途中の return でバッファが先に落ち、winmm が解放済みのメモリに書く恐れがあった）
    let mut data = vec![[0i16; voice::FRAME_SAMPLES]; BUFFERS];
    let mut headers: Vec<WAVEHDR> = data
        .iter_mut()
        .map(|buf| WAVEHDR {
            lpData: PSTR(buf.as_mut_ptr().cast()),
            dwBufferLength: (voice::FRAME_SAMPLES * 2) as u32,
            ..Default::default()
        })
        .collect();
    let mut hwi = HWAVEIN::default();
    // SAFETY: format はこの呼び出しの間生きている。CALLBACK_NULL なので終わりは WHDR_DONE を見る
    let rc = unsafe {
        waveInOpen(
            Some(&mut hwi),
            WAVE_MAPPER,
            &format,
            None,
            None,
            CALLBACK_NULL,
        )
    };
    if rc != 0 {
        log(format!("voice: waveInOpen = {rc}"));
        return Err(Outcome::Failed("マイクを開けなかったよ"));
    }
    // headers の Vec はこの後で伸び縮みさせないので、先頭のポインタは Mic が落ちるまで有効
    let mic = Mic {
        handle: hwi,
        headers: headers.as_mut_ptr(),
        count: headers.len(),
    };
    for h in headers.iter_mut() {
        // SAFETY: h と中身のバッファは headers・data が生きている間有効。Mic の drop より後まで生きる
        let ok = unsafe {
            waveInPrepareHeader(mic.handle, h, hdr_size) == 0
                && waveInAddBuffer(mic.handle, h, hdr_size) == 0
        };
        if !ok {
            return Err(Outcome::Failed("マイクを開けなかったよ"));
        }
    }
    // SAFETY: 開いたマイク
    if unsafe { waveInStart(mic.handle) } != 0 {
        return Err(Outcome::Failed("マイクを開けなかったよ"));
    }
    let mut next = 0;
    // 実時間の期限。区切りの時間は届いた音で進むので、マイクが音を返さなくなると止まらない（2026-10-24 検証係の指摘）
    let deadline = Instant::now() + Duration::from_millis(voice::MAX_RECORD_MS + 3_000);
    let reason = loop {
        if !ours() {
            break Err(Outcome::Cancelled);
        }
        if STOP_REQUESTED.load(Ordering::SeqCst) {
            break Ok(None);
        }
        let h = &mut headers[next];
        // WAVEHDR は詰めた構造体（packed）なので参照を取らずに読む。winmm が別スレッドで書くので、
        // 毎回メモリから読み直す（コンパイラに読みをまとめさせない）
        std::sync::atomic::compiler_fence(Ordering::SeqCst);
        // SAFETY: h は有効な WAVEHDR。位置が揃っていない読みなので read_unaligned
        let flags = unsafe { std::ptr::addr_of!(h.dwFlags).read_unaligned() };
        if flags & WHDR_DONE == 0 {
            if Instant::now() >= deadline {
                break Err(Outcome::Failed("マイクが止まったよ"));
            }
            std::thread::sleep(POLL);
            continue;
        }
        let n = (h.dwBytesRecorded as usize / 2).min(voice::FRAME_SAMPLES);
        let frame = data_of(h, n);
        let ended = on_frame(&frame, all);
        h.dwFlags &= !WHDR_DONE;
        // SAFETY: 準備済みの頭を返す
        if unsafe { waveInAddBuffer(mic.handle, h, hdr_size) } != 0 {
            break Err(Outcome::Failed("マイクが止まったよ"));
        }
        next = (next + 1) % BUFFERS;
        if let Some(r) = ended {
            break Ok(Some(r));
        }
    };
    // 止めて、返ってきた頭を外してから閉じる（Mic の drop。data・headers より先）
    drop(mic);
    drop(headers);
    drop(data);
    reason
}

/// 既定の録音デバイス（WAVE_MAPPER が使うもの）がミュートか。読めなければ None（録音は続ける）。
fn default_mic_muted() -> Option<bool> {
    // SAFETY: このスレッドで COM を使う間だけ初期化し、初期化できた時だけ戻す。インターフェイスは
    // CoUninitialize より前にこのブロックの中で落ちる
    unsafe {
        let init = CoInitializeEx(None, COINIT_MULTITHREADED);
        let muted = (|| {
            let e: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let dev = e.GetDefaultAudioEndpoint(eCapture, eConsole).ok()?;
            let vol: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None).ok()?;
            vol.GetMute().ok().map(|b| b.as_bool())
        })();
        if init.is_ok() {
            CoUninitialize();
        }
        muted
    }
}

/// 頭が指すバッファから n サンプルを写す。
fn data_of(h: &WAVEHDR, n: usize) -> Vec<i16> {
    // SAFETY: lpData は FRAME_SAMPLES 個の i16 を持つバッファを指し、n はそれ以下
    unsafe { std::slice::from_raw_parts(h.lpData.0.cast::<i16>(), n).to_vec() }
}

/// 開いたマイク。落とすと、止めて（渡した頭が全部返る）、頭を外してから閉じる。
/// バッファと頭より後に作ること（先に落ちて、winmm が触らなくなってからバッファが捨てられるように）。
struct Mic {
    handle: HWAVEIN,
    headers: *mut WAVEHDR,
    count: usize,
}

impl Drop for Mic {
    fn drop(&mut self) {
        let size = size_of::<WAVEHDR>() as u32;
        // SAFETY: waveInOpen が返したハンドルを 1 回だけ閉じる。headers は Mic より長く生きる Vec の先頭で、
        // reset の後は winmm が触らない。準備していない頭を外しても害は無い（エラーが返るだけ）
        unsafe {
            let _ = waveInReset(self.handle);
            for i in 0..self.count {
                let _ = waveInUnprepareHeader(self.handle, self.headers.add(i), size);
            }
            let _ = waveInClose(self.handle);
        }
    }
}

/// 文字起こしの子を起こして結果を読む。WAV のパスは `path` に後から届く（届かなければ子は何もせず終わる）。
fn transcribe(generation: u64, path: std::sync::mpsc::Receiver<Vec<u8>>, cwd: &Path) -> Outcome {
    let ours = ours_for(generation);
    let (Some(python), script) = (stt_python(), stt_script()) else {
        return Outcome::Failed("音声入力の準備が要るよ（設定を見てね）");
    };
    if !script.is_file() {
        return Outcome::Failed("音声入力の準備が要るよ（設定を見てね）");
    }
    let mut env: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| {
            // Python の挙動を変える変数は渡さない（-I でも PYTHONHOME などは効くものがある）
            !k.to_ascii_uppercase().starts_with("PYTHON")
        })
        .collect();
    env.push(("HF_HUB_OFFLINE".into(), "1".into()));
    env.push(("PYTHONIOENCODING".into(), "utf-8".into()));
    let register = move |job: isize| match RUNNING.lock() {
        Ok(mut r) if r.map(|(g, _)| g) == Some(generation) => {
            *r = Some((generation, job));
            true
        }
        _ => false,
    };
    let unregister = move || {
        if let Ok(mut r) = RUNNING.lock()
            && r.map(|(g, _)| g) == Some(generation)
        {
            *r = Some((generation, 0));
        }
    };
    let result = run_child(ChildRun {
        tag: "stt",
        start_failed: "文字起こしを起こせなかったよ",
        exe: python,
        args: vec!["-I".into(), script.to_string_lossy().into_owned()],
        env,
        cwd: cwd.to_path_buf(),
        input: Input::Later(path),
        timeout: stt_timeout(),
        register: &register,
        unregister: &unregister,
        wanted: &ours,
    });
    match result {
        Ok(out) => {
            if out.code != Some(0) {
                log(format!("stt: exit={:?}", out.code));
                return Outcome::Failed("文字起こしがうまくいかなかったよ");
            }
            let text = serde_json::from_slice::<serde_json::Value>(&out.stdout)
                .ok()
                .and_then(|v| v["text"].as_str().map(str::to_owned));
            match text.as_deref().and_then(voice::transcript_text) {
                Some(t) => Outcome::Text(t),
                None if text.is_some() => Outcome::Failed(voice::PHRASE_NOT_HEARD),
                None => Outcome::Failed("文字起こしがうまくいかなかったよ"),
            }
        }
        Err(Stop::Cancelled) => Outcome::Cancelled,
        Err(Stop::TimedOut) => Outcome::Failed("文字起こしが終わらなかったよ"),
        Err(Stop::Failed(why)) => Outcome::Failed(why),
    }
}

fn release(generation: u64) {
    if let Ok(mut r) = RUNNING.lock()
        && r.map(|(g, _)| g) == Some(generation)
    {
        *r = None;
    }
}

/// 文字起こしの Python（`scripts/setup-stt.ps1` が作る環境）。試験では `PET_TEST_STT_EXE` で差し替え。
fn stt_python() -> Option<PathBuf> {
    if test_overrides()
        && let Some(p) = std::env::var_os("PET_TEST_STT_EXE")
    {
        return (!p.is_empty()).then(|| PathBuf::from(p));
    }
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    let exe = local.join(r"pet-claude\stt\venv\Scripts\python.exe");
    exe.is_file().then_some(exe)
}

/// 文字起こしのスクリプトの中身（ビルドの時に exe へ埋め込む）。
const STT_SCRIPT: &str = include_str!("../../../scripts/stt/transcribe.py");

/// 文字起こしのスクリプト。exe に埋め込んだ中身を `%LOCALAPPDATA%\pet-claude\stt\transcribe.py` に書き出して使う。
///
/// ビルドした時のリポジトリのパス（`CARGO_MANIFEST_DIR`）を指すと、exe を動かした後にその場所へ置かれた
/// 別のファイルを実行してしまう（2026-10-23 セキュリティレビューの指摘）。書き出す先は自分のユーザーだけの
/// データフォルダで、毎回中身を埋め込みのものに揃える。書き出せなければ、無いものとして扱う。
fn stt_script() -> PathBuf {
    if test_overrides()
        && let Some(p) = std::env::var_os("PET_TEST_STT_SCRIPT")
    {
        return PathBuf::from(p);
    }
    let path = data_dir().join("stt").join("transcribe.py");
    let current = std::fs::read_to_string(&path).ok();
    if current.as_deref() != Some(STT_SCRIPT) {
        let written = std::fs::create_dir_all(path.parent().unwrap_or(&path))
            .and_then(|()| std::fs::write(&path, STT_SCRIPT));
        if let Err(e) = written {
            log(format!("stt: スクリプトを書き出せない: {e}"));
            return PathBuf::new();
        }
    }
    path
}

fn stt_timeout() -> Duration {
    let secs = test_overrides()
        .then(|| std::env::var("PET_STT_TIMEOUT_SECS").ok())
        .flatten()
        .and_then(|s| s.parse().ok());
    secs.map_or(STT_TIMEOUT, Duration::from_secs)
}

fn test_wav() -> Option<PathBuf> {
    test_overrides()
        .then(|| std::env::var_os("PET_TEST_VOICE_WAV"))
        .flatten()
        .map(PathBuf::from)
}

/// 試験の WAV（16kHz・モノラル・16bit）からサンプルを読む。
fn read_test_wav(path: &Path) -> Option<Vec<i16>> {
    let bytes = std::fs::read(path).ok()?;
    let info = voice::parse_wav(&bytes)?;
    if info.channels != 1 || info.sample_rate != voice::SAMPLE_RATE {
        return None;
    }
    // data チャンクは末尾にある前提（wav_bytes と同じ形）
    let start = bytes.len().checked_sub(info.data_len)?;
    Some(
        bytes[start..start + info.data_len]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| i16::from_le_bytes(*c))
            .collect(),
    )
}
