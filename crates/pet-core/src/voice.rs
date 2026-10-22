//! 声まわり（読み上げ・音声入力）の純粋な関数。
//!
//! 話す文の選び方、話す順番、録音の区切り、文字起こし結果の整形、WAV の組み立てと検査を持つ。
//! OS にもファイルにもネットワークにも触らない。時刻は引数のミリ秒（単調増加）で受け取る。
//! 読み上げる文も文字起こしの結果も外（モデル・CLI・Whisper）から来る信頼できないデータなので、
//! どの経路でも panic しない。
//! 仕様の正本は `.steering/20261022-phase3-voice/core-requirements.md`。

use std::collections::HashMap;

use crate::chat::{BUBBLE_HIDDEN, bubble_text, input_text};
use crate::hook::sanitize_line;

/// 録音のサンプリング周波数（Hz）。Whisper の入力に合わせた 16kHz のモノラル。
pub const SAMPLE_RATE: u32 = 16_000;

/// 録音の 1 回分（30ms）のサンプル数。
pub const FRAME_SAMPLES: usize = 480;

/// 同じ固定文言を話し直すまでの最短の間隔（ms）。
/// 並行する複数セッションから同じ出来事が続けて来ても、同じ一言を連呼しない長さ。
pub const SAME_PHRASE_COOLDOWN_MS: u64 = 15_000;

/// 録音の頭で静かさ（背景音の大きさ）を測る長さ（ms）。
pub const CALIBRATION_MS: u64 = 300;

/// 話していたと見なすのに要る、話している音の合計の長さ（ms）。咳や物音で始めない。
pub const MIN_SPEECH_MS: u64 = 300;

/// 話した後にこれだけ黙ったら録音を終える（ms）。短いと文の途中で切れる。
pub const END_SILENCE_MS: u64 = 1_000;

/// 一度も話さないまま待つ最長（ms）。黙ったまま録り続けない。
pub const NO_SPEECH_MS: u64 = 5_000;

/// 録音の最長（ms）。話し続けても必ずここで止める。
pub const MAX_RECORD_MS: u64 = 15_000;

/// 話していると見なす音量の下限（i16 の RMS）。静かな部屋の暗騒音を拾わない値。
pub const SPEECH_RMS_MIN: f32 = 300.0;

/// 静かさの何倍で「話している」か。背景音が大きい部屋でも、それより目立つ音だけを話し声にする。
pub const SPEECH_RMS_FACTOR: f32 = 3.0;
/// 録音の最大振幅（絶対値）がこれ以下なら、マイクが止められている（ミュート・プライバシー設定）と見なす。
/// 2026-10-22 K の PC でミュートの「マイク配列」から 6 秒録ると最大振幅 1・RMS 0.5 だった（完全な 0 ではない）。
/// 生きているマイクは静かな部屋でも数十はある
pub const MUTED_PEAK_MAX: i16 = 8;

/// 作業が終わった時の一言。
pub const PHRASE_DONE: &str = "できたよ";
/// 失敗した時の一言。
pub const PHRASE_FAILED: &str = "失敗しちゃった";
/// 入力待ちになった時の一言。
pub const PHRASE_WAITING: &str = "入力を待ってるよ";
/// 許可の確認が要る時の一言。
pub const PHRASE_PERMISSION: &str = "確認してほしいことがあるよ";
/// チャットが失敗した時の一言。
pub const PHRASE_CHAT_FAILED: &str = "うまくいかなかったよ";
/// 録音から文字が取れなかった時の一言。
pub const PHRASE_NOT_HEARD: &str = "聞き取れなかったよ";
/// 答えを吹き出しに出せない（URL・パスなどを含む）時の一言。中身はチャット欄で見られる。
pub const ANSWER_IN_PANEL: &str = "答えはチャット欄に出したよ";

/// 文字起こしの幻聴（句読点・空白を除いた形で完全一致したら捨てる）。
///
/// Whisper は無音や雑音に対してこの決まり文句を作ることがあり、Claude に送ると誤動作になる。
pub const HALLUCINATIONS: &[&str] = &[
    "ご視聴ありがとうございました",
    "ご視聴ありがとうございます",
    "チャンネル登録お願いします",
    "チャンネル登録をお願いします",
    "チャンネル登録よろしくお願いします",
    "最後までご視聴いただきありがとうございました",
];

/// 幻聴の照合で無視する句読点・記号。
const IGNORED_PUNCT: &str = "。、．，.,!！?？・…「」『』（）()";

/// チャットの答えから話す 1 行を選ぶ。話さない（空の答え・読む所が無い）時は `None`。
///
/// 吹き出しと同じ検査（`bubble_text`）を通すので、URL・パス・コード・秘密らしい文字列は読み上げない。
/// 検査で伏せられた時は、中身を読まずに「チャット欄に出した」と伝える。
pub fn speech_for_answer(answer: &str) -> Option<String> {
    if sanitize_line(answer, usize::MAX).is_empty() {
        return None;
    }
    let b = bubble_text(answer);
    if b == BUBBLE_HIDDEN {
        return Some(ANSWER_IN_PANEL.to_string());
    }
    // 140 字で切れた印の `…` は声に出さない
    let body = b.strip_suffix('…').unwrap_or(&b).trim();
    (!body.is_empty()).then(|| body.to_string())
}

/// 話す内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Utterance {
    /// 固定文言（Claude Code の出来事など）。同じものは続けて話さない。
    Phrase(String),
    /// チャットの答え（`speech_for_answer` を通した文）。固定文言より優先する。
    Answer(String),
}

/// 話す順番。待たせるのは 1 件だけで、溜めない。
#[derive(Debug, Default)]
pub struct SpeechQueue {
    /// 話し中か。
    speaking: bool,
    /// 次に話す 1 件。
    waiting: Option<Utterance>,
    /// 固定文言ごとに、最後に話し始めた時刻（ms）。
    last_started: HashMap<String, u64>,
}

impl SpeechQueue {
    /// 何も待っていない・話し中でない状態で作る。
    pub fn new() -> Self {
        Self::default()
    }

    /// 話す内容を積む。
    ///
    /// 固定文言は、話し中・冷却中・答えが待っている時は捨てる。答えは話し中でも受け、
    /// 待っている 1 件を置き換える（古い答えより新しい答えを話す）。
    pub fn push(&mut self, u: Utterance, now_ms: u64) {
        match u {
            Utterance::Phrase(t) => {
                let t = t.trim();
                if t.is_empty() || self.speaking {
                    return;
                }
                let cooling = self
                    .last_started
                    .get(t)
                    .is_some_and(|&last| now_ms.saturating_sub(last) < SAME_PHRASE_COOLDOWN_MS);
                if cooling || matches!(self.waiting, Some(Utterance::Answer(_))) {
                    return;
                }
                self.waiting = Some(Utterance::Phrase(t.to_string()));
            }
            Utterance::Answer(t) => {
                let t = t.trim();
                if t.is_empty() {
                    return;
                }
                self.waiting = Some(Utterance::Answer(t.to_string()));
            }
        }
    }

    /// 次に話す文を取り出して話し中にする。話し中・待ちが無い時は `None`。
    ///
    /// 固定文言なら、冷却の起点としてこの時刻を記録する。
    pub fn next(&mut self, now_ms: u64) -> Option<String> {
        if self.speaking {
            return None;
        }
        let u = self.waiting.take()?;
        self.speaking = true;
        match u {
            Utterance::Phrase(t) => {
                self.last_started.insert(t.clone(), now_ms);
                Some(t)
            }
            Utterance::Answer(t) => Some(t),
        }
    }

    /// 話し終えた。待っている 1 件は残す。
    pub fn finished(&mut self, _now_ms: u64) {
        self.speaking = false;
    }

    /// 待っている 1 件を捨て、話し中を解く。冷却の記録は残す（止めた直後に同じ一言を連呼しない）。
    pub fn clear(&mut self) {
        self.waiting = None;
        self.speaking = false;
    }

    /// 話し中か。
    pub fn speaking(&self) -> bool {
        self.speaking
    }
}

/// 1 フレームの二乗平均平方根（i16 の振幅）。空なら `0.0`。
pub fn rms(frame: &[i16]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    // 長いフレームでも精度が落ちないよう f64 で足す
    let sum: f64 = frame.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    (sum / frame.len() as f64).sqrt() as f32
}

/// 録音を終えた理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// 話して、十分黙った。
    Done,
    /// 一度も話さないまま待ち時間が尽きた。
    NoSpeech,
    /// 最長に達した。
    MaxLength,
}

/// 録音の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointState {
    /// 録音を続ける。
    Listening,
    /// 録音を終える。
    Ended(EndReason),
}

/// 録音の区切りを決める。フレームを順に渡し、話し終わり・無音・最長を見つけて知らせる。
#[derive(Debug)]
pub struct Endpointer {
    /// 受けたサンプルの合計。経過時間はここから数える。
    total_samples: u64,
    /// 静かさを測る間のフレームの RMS の最小値。測る間のフレームが無ければ `None`。
    quiet: Option<f32>,
    /// 「話している」フレームのサンプルの合計。
    speech_samples: u64,
    /// 「黙っている」フレームが続いたサンプル数。
    silence_samples: u64,
    /// 話していたと見なせたか（一度 true になったらそのまま）。
    heard: bool,
    /// 終わった理由。終わった後は何も変えない。
    ended: Option<EndReason>,
}

impl Endpointer {
    /// 録音の頭の状態で作る。
    pub fn new() -> Self {
        Self {
            total_samples: 0,
            quiet: None,
            speech_samples: 0,
            silence_samples: 0,
            heard: false,
            ended: None,
        }
    }

    /// 1 フレームを渡して、録音を続けるか終えるかを返す。
    ///
    /// 長さは `FRAME_SAMPLES` でなくてもよい。空のフレームは何も変えない。
    pub fn push(&mut self, frame: &[i16]) -> EndpointState {
        if let Some(r) = self.ended {
            return EndpointState::Ended(r);
        }
        if frame.is_empty() {
            return EndpointState::Listening;
        }
        let len = frame.len() as u64;
        self.total_samples += len;
        let level = rms(frame);

        if self.elapsed_ms() <= CALIBRATION_MS {
            // 話し始めが早くても静かさが上がりにくいよう、平均でなく最小値を取る
            self.quiet = Some(self.quiet.map_or(level, |q| q.min(level)));
        } else {
            let threshold = SPEECH_RMS_MIN.max(SPEECH_RMS_FACTOR * self.quiet.unwrap_or(0.0));
            if level >= threshold {
                self.speech_samples += len;
                self.silence_samples = 0;
            } else {
                self.silence_samples += len;
            }
            if samples_to_ms(self.speech_samples) >= MIN_SPEECH_MS {
                self.heard = true;
            }
        }

        let elapsed = self.elapsed_ms();
        let reason = if self.heard && samples_to_ms(self.silence_samples) >= END_SILENCE_MS {
            Some(EndReason::Done)
        } else if !self.heard && elapsed >= NO_SPEECH_MS {
            Some(EndReason::NoSpeech)
        } else if elapsed >= MAX_RECORD_MS {
            Some(EndReason::MaxLength)
        } else {
            None
        };
        match reason {
            Some(r) => {
                self.ended = Some(r);
                EndpointState::Ended(r)
            }
            None => EndpointState::Listening,
        }
    }

    /// 話し声を十分な長さ聞いたか。
    pub fn heard_speech(&self) -> bool {
        self.heard
    }

    /// 受けたサンプルの合計から数えた経過時間（ms・切り捨て）。
    pub fn elapsed_ms(&self) -> u64 {
        samples_to_ms(self.total_samples)
    }
}

impl Default for Endpointer {
    fn default() -> Self {
        Self::new()
    }
}

/// サンプル数を ms にする（切り捨て）。
fn samples_to_ms(samples: u64) -> u64 {
    samples * 1000 / u64::from(SAMPLE_RATE)
}

/// 文字起こしの結果を、Claude に送れる文に整える。送らない（空・幻聴・長すぎ）時は `None`。
pub fn transcript_text(raw: &str) -> Option<String> {
    let line = sanitize_line(raw, usize::MAX);
    let key: String = line
        .chars()
        .filter(|c| !c.is_whitespace() && !IGNORED_PUNCT.contains(*c))
        .collect();
    if key.is_empty() || HALLUCINATIONS.contains(&key.as_str()) {
        return None;
    }
    input_text(&line)
}

/// 16 ビット・モノラルの PCM を WAV にする（44 バイトの頭＋データ）。
///
/// データ長は `u32` に収まる範囲（録音は最長 15 秒）を前提にする。
pub fn wav_bytes(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // モノラル
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes()); // ブロック
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// WAV の中身のうち、鳴らす前に確かめたい項目。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavInfo {
    /// チャンネル数（1 か 2）。
    pub channels: u16,
    /// サンプリング周波数（Hz）。
    pub sample_rate: u32,
    /// 1 サンプルのビット数（16 だけを受ける）。
    pub bits_per_sample: u16,
    /// データ部の長さ（バイト）。
    pub data_len: usize,
}

fn le_u16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn le_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// WAV を検査する。16 ビット PCM のモノラル・ステレオで、`fmt ` の後に中身のある `data` が収まっていれば
/// `Some`。壊れた音を鳴らさないための検査で、音声エンジンの返事は外から来るので panic しない。
pub fn parse_wav(bytes: &[u8]) -> Option<WavInfo> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut fmt: Option<(u16, u32, u16)> = None;
    let mut pos = 12usize;
    // チャンクの頭（id 4 バイト＋長さ 4 バイト）が足りなくなったら終わり
    while pos.checked_add(8).is_some_and(|end| end <= bytes.len()) {
        let id = &bytes[pos..pos + 4];
        let size = le_u32(bytes, pos + 4) as usize;
        let body = pos + 8;
        let fits = body.checked_add(size).is_some_and(|end| end <= bytes.len());
        match id {
            b"fmt " => {
                if size < 16 || body + 16 > bytes.len() {
                    return None;
                }
                let format = le_u16(bytes, body);
                let channels = le_u16(bytes, body + 2);
                let sample_rate = le_u32(bytes, body + 4);
                let bits = le_u16(bytes, body + 14);
                if format != 1 || !(channels == 1 || channels == 2) || bits != 16 {
                    return None;
                }
                fmt = Some((channels, sample_rate, bits));
            }
            b"data" => {
                let (channels, sample_rate, bits_per_sample) = fmt?;
                if size == 0 || !fits {
                    return None;
                }
                return Some(WavInfo {
                    channels,
                    sample_rate,
                    bits_per_sample,
                    data_len: size,
                });
            }
            _ => {
                if !fits {
                    return None;
                }
            }
        }
        // 長さが奇数なら中身の後に 1 バイトの詰め物がある
        pos = body.saturating_add(size).saturating_add(size & 1);
    }
    None
}

/// 録音がマイクを止められた時の形か（話していないうえに、最大振幅が `MUTED_PEAK_MAX` 以下）。空なら false。
pub fn looks_muted(samples: &[i16]) -> bool {
    !samples.is_empty()
        && samples
            .iter()
            .all(|&s| s.unsigned_abs() <= MUTED_PEAK_MAX.unsigned_abs())
}
