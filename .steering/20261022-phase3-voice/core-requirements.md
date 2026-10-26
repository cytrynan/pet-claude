---
type: Steering
description: pet-core::voice（読み上げ・音声入力の純粋ロジック）の要求。テスト作成係と実装係の唯一の原本
generated: { by: kuroko/claude-opus-5-5, at: 2026-10-22T10:00:00+09:00 }
---

# 要求: `pet-core::voice`（声まわりの純粋な関数）

デスクトップのペット（pet-app）は、チャットの答えや Claude Code の出来事を AivisSpeech（音声合成エンジン）で読み上げ、
バーの音声ボタンで録音した声を文字起こししてチャットに送る。このモジュールは、その**話す文の選び方・話す順番・録音の区切り・
文字起こし結果の整形・WAV の組み立てと検査**を持つ。**OS にもファイルにもネットワークにも触らない純粋な関数**にする。
時刻は引数のミリ秒（単調増加）で受け取る。

ファイル: `crates/pet-core/src/voice.rs`（`pub mod voice;` を `lib.rs` に足す）、テストは `crates/pet-core/tests/voice.rs`。
依存は既存のものだけ。既存の `pet_core::chat::bubble_text(s)`・`pet_core::chat::BUBBLE_HIDDEN`・`pet_core::chat::input_text(s)`・
`pet_core::hook::sanitize_line(s, max_chars)` を使ってよい（挙動は下の「既存の関数」）。

## 用途と脅威

- 読み上げる文は外（モデル・CLI）から来る。**吹き出しと同じ検査（N3: 1〜140 字・単一行・URL/パス/コード/秘密らしい文字列は出さない）を通した文しか話さない**
- Claude Code の出来事は並行の複数セッションから続けて来る。**同じ固定文言を何度も話し続けない・溜めない**
- 録音は K のマイク。**区切りを誤ると、黙っても止まらない（最長まで録る）か、話し始める前に切れる**
- 文字起こし（Whisper）は無音や雑音で決まり文句を作る（幻聴）。**幻聴を Claude に送らない**
- AivisSpeech が壊れた音を返したら鳴らさない

## 既存の関数（使ってよい・変えない）

- `chat::bubble_text(s) -> String`: 1 行に整え（改行・制御文字は空白、空白の並びは 1 つ、前後を落とす）、空なら `"（答えが空でした）"`、URL・パス・コード・秘密らしい文字列があれば `BUBBLE_HIDDEN`（`"（答えはチャット欄に出しました）"`）、140 字を超えたら 139 字＋`…`
- `chat::input_text(s) -> Option<String>`: 前後の空白を落とし、空か 2000 字超なら `None`
- `hook::sanitize_line(s, max)`: 制御文字を空白に、空白の並びを半角 1 つに、前後の空白を落とし、`max` 字を超えたら `max-1` 字＋`…`

## 識別子契約（この名前・型で作る）

```rust
pub const SAMPLE_RATE: u32 = 16_000;
/// 録音の 1 回分（30ms）
pub const FRAME_SAMPLES: usize = 480;
pub const SAME_PHRASE_COOLDOWN_MS: u64 = 15_000;
pub const CALIBRATION_MS: u64 = 300;
pub const MIN_SPEECH_MS: u64 = 300;
pub const END_SILENCE_MS: u64 = 1_000;
pub const NO_SPEECH_MS: u64 = 5_000;
pub const MAX_RECORD_MS: u64 = 15_000;
/// 話していると見なす音量の下限（i16 の RMS）
pub const SPEECH_RMS_MIN: f32 = 300.0;
/// 静かさの何倍で「話している」か
pub const SPEECH_RMS_FACTOR: f32 = 3.0;

pub const PHRASE_DONE: &str = "できたよ";
pub const PHRASE_FAILED: &str = "失敗しちゃった";
pub const PHRASE_WAITING: &str = "入力を待ってるよ";
pub const PHRASE_PERMISSION: &str = "確認してほしいことがあるよ";
pub const PHRASE_CHAT_FAILED: &str = "うまくいかなかったよ";
pub const PHRASE_NOT_HEARD: &str = "聞き取れなかったよ";
pub const ANSWER_IN_PANEL: &str = "答えはチャット欄に出したよ";

/// 文字起こしの幻聴（句読点・空白を除いた形で完全一致したら捨てる）
pub const HALLUCINATIONS: &[&str] = &[
    "ご視聴ありがとうございました",
    "ご視聴ありがとうございます",
    "チャンネル登録お願いします",
    "チャンネル登録をお願いします",
    "チャンネル登録よろしくお願いします",
    "最後までご視聴いただきありがとうございました",
];

pub fn speech_for_answer(answer: &str) -> Option<String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Utterance { Phrase(String), Answer(String) }

#[derive(Debug, Default)]
pub struct SpeechQueue { /* 中身は自由 */ }
impl SpeechQueue {
    pub fn new() -> Self;
    pub fn push(&mut self, u: Utterance, now_ms: u64);
    pub fn next(&mut self, now_ms: u64) -> Option<String>;
    pub fn finished(&mut self, now_ms: u64);
    pub fn clear(&mut self);
    pub fn speaking(&self) -> bool;
}

pub fn rms(frame: &[i16]) -> f32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason { Done, NoSpeech, MaxLength }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointState { Listening, Ended(EndReason) }

#[derive(Debug)]
pub struct Endpointer { /* 中身は自由 */ }
impl Endpointer {
    pub fn new() -> Self;
    pub fn push(&mut self, frame: &[i16]) -> EndpointState;
    pub fn heard_speech(&self) -> bool;
    pub fn elapsed_ms(&self) -> u64;
}

pub fn transcript_text(raw: &str) -> Option<String>;

pub fn wav_bytes(samples: &[i16], sample_rate: u32) -> Vec<u8>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavInfo { pub channels: u16, pub sample_rate: u32, pub bits_per_sample: u16, pub data_len: usize }
pub fn parse_wav(bytes: &[u8]) -> Option<WavInfo>;
```

## 1. `speech_for_answer`（チャットの答えから話す 1 行）

1. `answer` を `sanitize_line(answer, usize::MAX)` に通して空なら `None`（空の答えは話さない）
2. `b = chat::bubble_text(answer)`
3. `b == BUBBLE_HIDDEN` なら `Some(ANSWER_IN_PANEL)`
4. それ以外は `b` の末尾の `…`（1 字）を落とし、前後の空白を落とした文。空になったら `None`。それ以外は `Some(その文)`

## 2. `SpeechQueue`（話す順番）

状態: 「話し中か」・「待っている 1 件（無いか、`Phrase` か `Answer`）」・「固定文言ごとに最後に話し始めた時刻」。

- `push(Phrase(t), now)`:
  - `t` の前後の空白を落として空なら何もしない
  - 話し中なら捨てる
  - 同じ文言を最後に話し始めてから `SAME_PHRASE_COOLDOWN_MS` 未満なら捨てる（`now - 最後 < 15000`。ちょうど 15000 なら受ける）
  - 待っている 1 件が `Answer` なら捨てる（答えが優先）
  - それ以外は待っている 1 件をこれにする（待っていた `Phrase` は捨てて新しい方にする）
- `push(Answer(t), now)`:
  - `t` の前後の空白を落として空なら何もしない
  - 話し中でも受ける。待っている 1 件を（`Phrase` でも `Answer` でも）これに置き換える。冷却時間は見ない
- `next(now)`: 話し中なら `None`。待っている 1 件が無ければ `None`。あれば取り出して話し中にし、文（前後の空白を落とした形）を返す。`Phrase` だったら、その文言の「最後に話し始めた時刻」を `now` にする
- `finished(now)`: 話し中を解く（話し中でなければ何もしない）。待っている 1 件は残す
- `clear()`: 待っている 1 件を捨て、話し中を解く。「最後に話し始めた時刻」は残す
- `speaking()`: 話し中か

`new()` と `Default` は同じ（何も待っていない・話し中でない）。

## 3. `rms` と `Endpointer`（録音の区切り）

- `rms(frame)`: 各サンプルを f32 にして二乗平均の平方根。空なら `0.0`
- 経過時間は受けたサンプル数から数える: `elapsed_ms = 受けたサンプルの合計 × 1000 / SAMPLE_RATE`（切り捨て）。`push` のたびにまず経過時間を進める
- **静かさを測る間**: そのフレームを受け終えた時点の `elapsed_ms` が `CALIBRATION_MS` 以下のフレームは、静かさの測定にだけ使う（話しているかは判定しない）。静かさ＝その間のフレームの `rms` の**最小値**（話し始めが早くても静かさが上がりにくいように）
- **判定のしきい値**: `max(SPEECH_RMS_MIN, SPEECH_RMS_FACTOR × 静かさ)`。測る間のフレームが 1 つも無かった時は静かさ 0
- 測る間の後のフレームは、`rms >= しきい値` なら「話している」フレーム、そうでなければ「黙っている」フレーム
- 「話している」フレームの長さ（サンプル数から ms）を合計し、`MIN_SPEECH_MS` 以上になったら `heard_speech()` が true（以後ずっと true）
- 「黙っている」フレームが続いた長さ（「話している」フレームで 0 に戻る）を数える
- `push` の戻り値（この順に見る）:
  1. 既に `Ended(r)` なら、何も数えずに同じ `Ended(r)` を返す（経過時間も進めない）
  2. `heard_speech()` が true で、黙っている長さ ≥ `END_SILENCE_MS` → `Ended(Done)`
  3. `heard_speech()` が false で、`elapsed_ms ≥ NO_SPEECH_MS` → `Ended(NoSpeech)`
  4. `elapsed_ms ≥ MAX_RECORD_MS` → `Ended(MaxLength)`（`heard_speech` が false なら 3 が先に当たる）
  5. それ以外 `Listening`
- 黙っている長さは「heard_speech が true になる前」の黙りも数えてよいが、2 の判定は heard_speech が true の時だけ（話し始める前の沈黙で Done にしない）。**heard_speech が true になった時点で、黙っている長さは直前の値のまま**（話している最中は 0 なので問題にならない）
- フレームの長さが `FRAME_SAMPLES` でなくても受ける（経過時間はサンプル数で数える）。空のフレームは経過時間を進めず、判定も変えない（戻り値は今の状態）

## 4. `transcript_text`（文字起こしの結果を送れる文に）

1. `line = sanitize_line(raw, usize::MAX)`
2. `key` = `line` から空白（Unicode の空白）と句読点 `。、．，.,!！?？・…「」『』（）()` を全部除いた文字列
3. `key` が空なら `None`
4. `key` が `HALLUCINATIONS` のどれかと完全一致なら `None`
5. `chat::input_text(&line)` を返す（2000 字を超えたら `None`）

## 5. `wav_bytes` と `parse_wav`

- `wav_bytes(samples, rate)`: 44 バイトの頭＋データ。リトルエンディアン。`RIFF`・`36 + データ長`・`WAVE`・`fmt `・16・形式 1（PCM）・チャンネル 1・`rate`・バイト毎秒 `rate × 2`・ブロック 2・16 ビット・`data`・データ長（`samples.len() × 2`）・サンプル
- `parse_wav(bytes)`:
  - 先頭 12 バイトが `RIFF`・(4 バイト何でもよい)・`WAVE` でなければ `None`
  - 12 バイト目からチャンクを順に読む（id 4 バイト＋長さ 4 バイト＋中身。長さが奇数なら中身の後に 1 バイトの詰め物）。途中で頭（8 バイト）が足りなくなったら終わり
  - `fmt ` チャンク: 長さ 16 以上。形式が 1（PCM）・チャンネル 1 か 2・ビット 16 でなければ `None`
  - `data` チャンク: `fmt ` の後に来ること（`fmt ` より前の `data` は `None`）。長さが 0、または中身が `bytes` の残りに収まらなければ `None`
  - `data` を見つけたらそこで読むのをやめて `Some(WavInfo { channels, sample_rate, bits_per_sample, data_len })`
  - `fmt ` か `data` が無いまま終わったら `None`
  - 知らないチャンク（`LIST` など）は飛ばす。中身が残りに収まらないチャンクがあれば `None`

## 受け入れ（テストで確かめること）

- 1: 普通の答え・空・空白だけ・URL を含む・140 字超（`…` が落ちる）・改行を含む
- 2: 話し中の固定文言を捨てる・冷却の境界（14999 と 15000）・答えが固定文言を押しのける・固定文言は答えを押しのけない・答えは話し中でも受ける・`next` の後の `finished` で次が出る・`clear`・空の文
- 3: 無音だけ（5 秒で NoSpeech）・話して黙る（Done。話した長さ 300ms 未満なら Done にならない）・話し続ける（15 秒で MaxLength）・最初から話している（静かさは最小値なので、しきい値が上がりすぎない）・Ended の後は変わらない・空のフレーム・小さい音（`SPEECH_RMS_MIN` 未満）は話していない
- 4: 幻聴（句読点つき・前後の空白つき）・句読点だけ・普通の文・2001 字
- 5: `wav_bytes` を `parse_wav` が読める・壊れた頭・`data` が短い・`LIST` を挟む・奇数長のチャンクの詰め物・形式 3（float）は `None`・`fmt ` より前の `data`
