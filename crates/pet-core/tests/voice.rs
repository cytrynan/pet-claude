//! `pet_core::voice` の受け入れテスト（テスト先行）。
//!
//! 原本: `.steering/20261022-phase3-voice/core-requirements.md` の識別子契約と 1〜5 節・受け入れ節。公開 API だけを使う。
//! 要求書で決まっていない点（questions.md の Q1〜）に依存する入力は避けている。
//!
//! 音のフレームは「+amp と -amp を交互に並べた矩形波」で作る。RMS がちょうど amp になるので、
//! しきい値（max(300, 3×静かさ)）の境界を整数の振幅で突ける。境界ちょうどの値（300・600・1200 など）は
//! f32 で二乗和が厳密に表せる振幅だけを選んでいる（Q13）。

use pet_core::chat::{BUBBLE_HIDDEN, bubble_text};
use pet_core::hook::sanitize_line;
use pet_core::voice::{
    ANSWER_IN_PANEL, CALIBRATION_MS, END_SILENCE_MS, EndReason, EndpointState, Endpointer,
    FRAME_SAMPLES, HALLUCINATIONS, MAX_RECORD_MS, MIN_SPEECH_MS, NO_SPEECH_MS, PHRASE_CHAT_FAILED,
    PHRASE_DONE, PHRASE_FAILED, PHRASE_NOT_HEARD, PHRASE_PERMISSION, PHRASE_WAITING,
    SAME_PHRASE_COOLDOWN_MS, SAMPLE_RATE, SPEECH_RMS_FACTOR, SPEECH_RMS_MIN, SpeechQueue,
    Utterance, WavInfo, parse_wav, rms, speech_for_answer, transcript_text, wav_bytes,
};

/// 話している音として十分に大きい振幅（どのしきい値も超える）。
const LOUD: i16 = 5000;

fn st(x: &str) -> String {
    x.to_string()
}

/// `pattern` を繰り返して、ちょうど `n` 字の文字列を作る。
fn cycle(pattern: &str, n: usize) -> String {
    pattern.chars().cycle().take(n).collect()
}

/// f32 で累積する実装の丸め誤差（相対 1e-4 程度）は許す。しきい値の境界は別の形で突く。
fn assert_close(actual: f32, expected: f32) {
    let tolerance = 0.01 + expected.abs() * 1e-4;
    assert!(
        (actual - expected).abs() < tolerance,
        "{actual} が {expected} に近くない"
    );
}

// ---------------------------------------------------------------------------
// 識別子契約（定数の値）
// ---------------------------------------------------------------------------

#[test]
fn constants_match_the_identifier_contract() {
    assert_eq!(SAMPLE_RATE, 16_000);
    assert_eq!(FRAME_SAMPLES, 480);
    assert_eq!(SAME_PHRASE_COOLDOWN_MS, 15_000);
    assert_eq!(CALIBRATION_MS, 300);
    assert_eq!(MIN_SPEECH_MS, 300);
    assert_eq!(END_SILENCE_MS, 1_000);
    assert_eq!(NO_SPEECH_MS, 5_000);
    assert_eq!(MAX_RECORD_MS, 15_000);
    assert_eq!(SPEECH_RMS_MIN, 300.0);
    assert_eq!(SPEECH_RMS_FACTOR, 3.0);
    assert_eq!(PHRASE_DONE, "できたよ");
    assert_eq!(PHRASE_FAILED, "失敗しちゃった");
    assert_eq!(PHRASE_WAITING, "入力を待ってるよ");
    assert_eq!(PHRASE_PERMISSION, "確認してほしいことがあるよ");
    assert_eq!(PHRASE_CHAT_FAILED, "うまくいかなかったよ");
    assert_eq!(PHRASE_NOT_HEARD, "聞き取れなかったよ");
    assert_eq!(ANSWER_IN_PANEL, "答えはチャット欄に出したよ");
    assert_eq!(HALLUCINATIONS.len(), 6);
    assert!(HALLUCINATIONS.contains(&"ご視聴ありがとうございました"));
    assert!(HALLUCINATIONS.contains(&"最後までご視聴いただきありがとうございました"));
}

// ---------------------------------------------------------------------------
// 1. speech_for_answer
// ---------------------------------------------------------------------------

/// 要求書 §1 の手順をそのまま書いた参照実装（既存の関数だけで組む）。
fn expected_speech(answer: &str) -> Option<String> {
    if sanitize_line(answer, usize::MAX).is_empty() {
        return None;
    }
    let b = bubble_text(answer);
    if b == BUBBLE_HIDDEN {
        return Some(st(ANSWER_IN_PANEL));
    }
    let s = b.strip_suffix('…').unwrap_or(&b).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

#[test]
fn answer_speech_plain_answer_is_spoken_as_is() {
    assert_eq!(
        speech_for_answer("テストが通ったよ"),
        Some(st("テストが通ったよ"))
    );
    assert_eq!(
        speech_for_answer("  前後に空白がある答え  "),
        Some(st("前後に空白がある答え"))
    );
}

#[test]
fn answer_speech_empty_answer_is_not_spoken() {
    assert_eq!(speech_for_answer(""), None);
    assert_eq!(speech_for_answer("   "), None);
    assert_eq!(speech_for_answer("\n\t \r\n"), None);
    // 制御文字だけも空と同じ（sanitize_line で空になる）。
    assert_eq!(speech_for_answer("\u{0007}\u{0001}"), None);
    // 吹き出しの「（答えが空でした）」を読み上げてはいけない。
    assert_ne!(speech_for_answer(""), Some(st("（答えが空でした）")));
}

#[test]
fn answer_speech_url_is_replaced_by_the_panel_notice() {
    for answer in [
        "https://example.com/secret?token=abc",
        "詳しくは https://example.com/docs を見てね",
        "http://localhost:3000 で動いています",
    ] {
        assert_eq!(
            speech_for_answer(answer),
            Some(st(ANSWER_IN_PANEL)),
            "{answer:?}"
        );
    }
    // 読み上げ用の文言は吹き出しの文言とは別物。
    assert_ne!(ANSWER_IN_PANEL, BUBBLE_HIDDEN);
}

#[test]
fn answer_speech_newlines_become_single_spaces() {
    assert_eq!(
        speech_for_answer("一行目\n二行目"),
        Some(st("一行目 二行目"))
    );
    assert_eq!(
        speech_for_answer("一行目\r\n\r\n  二行目\n"),
        Some(st("一行目 二行目"))
    );
}

#[test]
fn answer_speech_over_140_chars_drops_the_ellipsis() {
    // 140 字ちょうどは切られない。末尾の … もない。
    let exact = cycle("あいうえお", 140);
    let s = speech_for_answer(&exact).expect("140 字は話せる");
    assert_eq!(s.chars().count(), 140);
    assert!(!s.ends_with('…'));

    // 141 字以上は 139 字＋… になるので、… を落として 139 字。
    for n in [141, 200, 1000] {
        let s = speech_for_answer(&cycle("あいうえお", n)).expect("長い答えも話せる");
        assert_eq!(s.chars().count(), 139, "入力 {n} 字");
        assert!(!s.ends_with('…'), "入力 {n} 字");
        assert_eq!(s, cycle("あいうえお", 139), "入力 {n} 字");
    }
}

#[test]
fn answer_speech_trailing_ellipsis_is_dropped_even_when_not_truncated() {
    assert_eq!(speech_for_answer("考え中…"), Some(st("考え中")));
    // … を落とした後の前後の空白も落とす。
    assert_eq!(speech_for_answer("考え中 …"), Some(st("考え中")));
}

#[test]
fn answer_speech_only_ellipsis_is_not_spoken() {
    assert_eq!(speech_for_answer("…"), None);
    assert_eq!(speech_for_answer("  …  "), None);
}

#[test]
fn answer_speech_matches_the_reference_over_assorted_inputs() {
    let long = cycle("あいうえお", 300);
    let inputs = [
        "テストが通ったよ",
        "",
        "   ",
        "考え中…",
        "…",
        "https://example.com",
        "C:\\Users\\someone\\secret.txt を開いた",
        "/etc/passwd を読んだ",
        "```rust\nfn main() {}\n```",
        "`cargo test` を実行してね",
        "token=ghp_abcdefghijklmnopqrstuvwxyz0123456789",
        "AKIAIOSFODNN7EXAMPLE",
        "一行目\n二行目\n三行目",
        long.as_str(),
    ];
    for input in inputs {
        let got = speech_for_answer(input);
        assert_eq!(got, expected_speech(input), "入力 {input:?}");
        if let Some(s) = got {
            let n = s.chars().count();
            assert!(
                (1..=140).contains(&n),
                "読み上げが 1〜140 字でない: {n} 字 {s:?}"
            );
            assert!(
                !s.chars().any(|c| c.is_control()),
                "読み上げに制御文字が入っている: {s:?}"
            );
            assert_eq!(s, s.trim(), "前後に空白が残っている: {s:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// 2. SpeechQueue
// ---------------------------------------------------------------------------

fn phrase(t: &str) -> Utterance {
    Utterance::Phrase(st(t))
}

fn answer(t: &str) -> Utterance {
    Utterance::Answer(st(t))
}

/// `text` の固定文言を `t` ミリ秒に話し始め、同じ時刻に話し終えた状態のキュー。
fn queue_after_phrase(text: &str, t: u64) -> SpeechQueue {
    let mut q = SpeechQueue::new();
    q.push(phrase(text), t);
    assert_eq!(q.next(t), Some(st(text)));
    q.finished(t);
    assert!(!q.speaking());
    q
}

#[test]
fn queue_new_and_default_are_idle() {
    for mut q in [SpeechQueue::new(), SpeechQueue::default()] {
        assert!(!q.speaking());
        assert_eq!(q.next(0), None);
        assert_eq!(q.next(1_000), None);
        assert!(!q.speaking());
    }
}

#[test]
fn queue_speaking_starts_at_next_not_at_push() {
    let mut q = SpeechQueue::new();
    q.push(phrase(PHRASE_DONE), 0);
    assert!(!q.speaking());
    assert_eq!(q.next(0), Some(st(PHRASE_DONE)));
    assert!(q.speaking());
    q.finished(100);
    assert!(!q.speaking());
}

#[test]
fn queue_waiting_item_is_taken_exactly_once() {
    let mut q = SpeechQueue::new();
    q.push(answer("答えです"), 0);
    assert_eq!(q.next(0), Some(st("答えです")));
    assert_eq!(q.next(0), None);
    q.finished(10);
    assert_eq!(q.next(10), None);
    assert_eq!(q.next(20), None);
}

#[test]
fn queue_texts_are_trimmed() {
    let mut q = SpeechQueue::new();
    q.push(phrase("  できたよ \n"), 0);
    assert_eq!(q.next(0), Some(st("できたよ")));
    q.finished(1);
    q.push(answer("  答えだよ\t\n"), 2);
    assert_eq!(q.next(2), Some(st("答えだよ")));
}

#[test]
fn queue_blank_utterances_are_ignored() {
    let mut q = SpeechQueue::new();
    q.push(phrase(""), 0);
    q.push(phrase("  \t\n"), 0);
    q.push(answer(""), 0);
    q.push(answer(" \n "), 0);
    assert_eq!(q.next(0), None);
    assert!(!q.speaking());
}

#[test]
fn queue_blank_answer_does_not_replace_a_waiting_phrase() {
    let mut q = SpeechQueue::new();
    q.push(phrase(PHRASE_DONE), 0);
    q.push(answer("   "), 1);
    assert_eq!(q.next(1), Some(st(PHRASE_DONE)));
}

#[test]
fn queue_blank_phrase_does_not_replace_a_waiting_answer_or_phrase() {
    let mut q = SpeechQueue::new();
    q.push(answer("答え"), 0);
    q.push(phrase("  "), 1);
    assert_eq!(q.next(1), Some(st("答え")));
    q.finished(2);

    q.push(phrase(PHRASE_FAILED), 3);
    q.push(phrase(" \n"), 4);
    assert_eq!(q.next(4), Some(st(PHRASE_FAILED)));
}

#[test]
fn queue_blank_utterance_while_speaking_changes_nothing() {
    let mut q = SpeechQueue::new();
    q.push(phrase(PHRASE_DONE), 0);
    assert_eq!(q.next(0), Some(st(PHRASE_DONE)));
    q.push(answer("   "), 10);
    q.push(phrase(""), 10);
    q.finished(20);
    assert_eq!(q.next(20), None);
}

#[test]
fn queue_phrase_pushed_while_speaking_is_dropped() {
    let mut q = SpeechQueue::new();
    q.push(phrase(PHRASE_DONE), 0);
    assert_eq!(q.next(0), Some(st(PHRASE_DONE)));
    // 話し中は別の固定文言も、同じ文言も捨てる。
    q.push(phrase(PHRASE_FAILED), 100);
    q.push(phrase(PHRASE_DONE), 200);
    assert_eq!(q.next(300), None);
    q.finished(400);
    assert_eq!(q.next(400), None, "話し中に来た固定文言は溜めない");
}

#[test]
fn queue_first_phrase_is_accepted_at_time_zero() {
    // 「最後に話し始めた時刻」が無い間は冷却に掛からない（初期値 0 と取り違えない）。
    let mut q = SpeechQueue::new();
    q.push(phrase(PHRASE_WAITING), 0);
    assert_eq!(q.next(0), Some(st(PHRASE_WAITING)));
}

#[test]
fn queue_same_phrase_is_dropped_at_14999ms_after_start() {
    let mut q = queue_after_phrase(PHRASE_DONE, 0);
    q.push(phrase(PHRASE_DONE), SAME_PHRASE_COOLDOWN_MS - 1);
    assert_eq!(q.next(SAME_PHRASE_COOLDOWN_MS - 1), None);
    assert!(!q.speaking());
}

#[test]
fn queue_same_phrase_is_accepted_at_exactly_15000ms_after_start() {
    let mut q = queue_after_phrase(PHRASE_DONE, 0);
    q.push(phrase(PHRASE_DONE), SAME_PHRASE_COOLDOWN_MS);
    assert_eq!(q.next(SAME_PHRASE_COOLDOWN_MS), Some(st(PHRASE_DONE)));
}

#[test]
fn queue_cooldown_boundary_holds_with_a_nonzero_start_time() {
    let base = 123_456;
    let mut dropped = queue_after_phrase(PHRASE_FAILED, base);
    dropped.push(phrase(PHRASE_FAILED), base + 14_999);
    assert_eq!(dropped.next(base + 14_999), None);

    let mut accepted = queue_after_phrase(PHRASE_FAILED, base);
    accepted.push(phrase(PHRASE_FAILED), base + 15_000);
    assert_eq!(accepted.next(base + 15_000), Some(st(PHRASE_FAILED)));
}

#[test]
fn queue_cooldown_is_measured_from_when_speech_started_not_from_push() {
    // 0 に積み、1000 に話し始める。冷却は 1000 から数える。
    let mut q = SpeechQueue::new();
    q.push(phrase(PHRASE_DONE), 0);
    assert_eq!(q.next(1_000), Some(st(PHRASE_DONE)));
    q.finished(1_500);

    q.push(phrase(PHRASE_DONE), 15_000); // 15000 - 1000 = 14000 < 15000
    assert_eq!(q.next(15_000), None);
    q.push(phrase(PHRASE_DONE), 15_999); // 14999 < 15000
    assert_eq!(q.next(15_999), None);
    q.push(phrase(PHRASE_DONE), 16_000); // ちょうど 15000
    assert_eq!(q.next(16_000), Some(st(PHRASE_DONE)));
}

#[test]
fn queue_cooldown_is_per_phrase() {
    let mut q = queue_after_phrase(PHRASE_DONE, 0);
    q.push(phrase(PHRASE_FAILED), 100);
    assert_eq!(q.next(100), Some(st(PHRASE_FAILED)));
    q.finished(200);
    // それぞれの文言が自分の時刻で冷える。
    q.push(phrase(PHRASE_DONE), 14_999);
    assert_eq!(q.next(14_999), None);
    q.push(phrase(PHRASE_FAILED), 14_999);
    assert_eq!(
        q.next(14_999),
        None,
        "FAILED は 100 に始めたので 14899 しか経っていない"
    );
    q.push(phrase(PHRASE_DONE), 15_000);
    assert_eq!(q.next(15_000), Some(st(PHRASE_DONE)));
    q.finished(15_100);
    q.push(phrase(PHRASE_FAILED), 15_100);
    assert_eq!(q.next(15_100), Some(st(PHRASE_FAILED)));
}

#[test]
fn queue_later_phrase_replaces_a_waiting_phrase() {
    let mut q = SpeechQueue::new();
    q.push(phrase(PHRASE_DONE), 0);
    q.push(phrase(PHRASE_FAILED), 1);
    assert_eq!(q.next(2), Some(st(PHRASE_FAILED)));
    q.finished(3);
    assert_eq!(q.next(3), None, "押しのけられた固定文言は残らない");
    // 押しのけられた DONE は話し始めていないので冷却中ではない。
    q.push(phrase(PHRASE_DONE), 4);
    assert_eq!(q.next(4), Some(st(PHRASE_DONE)));
}

#[test]
fn queue_same_phrase_pushed_twice_while_waiting_is_spoken_once() {
    let mut q = SpeechQueue::new();
    q.push(phrase(PHRASE_DONE), 0);
    q.push(phrase(PHRASE_DONE), 1);
    assert_eq!(q.next(2), Some(st(PHRASE_DONE)));
    q.finished(3);
    assert_eq!(q.next(3), None);
}

#[test]
fn queue_cooled_down_phrase_does_not_replace_a_waiting_phrase() {
    let mut q = queue_after_phrase(PHRASE_DONE, 0);
    q.push(phrase(PHRASE_FAILED), 1_000); // 待つ
    q.push(phrase(PHRASE_DONE), 2_000); // 冷却中なので捨てる（待っている FAILED は残る）
    assert_eq!(q.next(2_000), Some(st(PHRASE_FAILED)));
}

#[test]
fn queue_answer_pushes_out_a_waiting_phrase() {
    let mut q = SpeechQueue::new();
    q.push(phrase(PHRASE_DONE), 0);
    q.push(answer("答えです"), 1);
    assert_eq!(q.next(2), Some(st("答えです")));
    q.finished(3);
    assert_eq!(q.next(3), None, "押しのけられた固定文言は残らない");
    // 押しのけられた DONE は話し始めていないので冷却中ではない。
    q.push(phrase(PHRASE_DONE), 4);
    assert_eq!(q.next(4), Some(st(PHRASE_DONE)));
}

#[test]
fn queue_phrase_does_not_push_out_a_waiting_answer() {
    let mut q = SpeechQueue::new();
    q.push(answer("答えです"), 0);
    q.push(phrase(PHRASE_DONE), 1);
    assert_eq!(q.next(2), Some(st("答えです")));
    q.finished(3);
    assert_eq!(q.next(3), None, "捨てられた固定文言は後から出てこない");
}

#[test]
fn queue_later_answer_replaces_a_waiting_answer() {
    let mut q = SpeechQueue::new();
    q.push(answer("最初の答え"), 0);
    q.push(answer("新しい答え"), 1);
    assert_eq!(q.next(2), Some(st("新しい答え")));
    q.finished(3);
    assert_eq!(q.next(3), None);
}

#[test]
fn queue_answer_is_accepted_while_speaking_and_comes_after_finished() {
    let mut q = SpeechQueue::new();
    q.push(answer("一つ目"), 0);
    assert_eq!(q.next(0), Some(st("一つ目")));
    assert!(q.speaking());

    q.push(answer("二つ目"), 100);
    assert!(q.speaking());
    assert_eq!(q.next(100), None, "話し中は次を出さない");

    q.finished(200);
    assert!(!q.speaking());
    assert_eq!(q.next(200), Some(st("二つ目")));
    assert!(q.speaking());
}

#[test]
fn queue_answer_while_speaking_a_phrase_waits_for_finished() {
    let mut q = SpeechQueue::new();
    q.push(phrase(PHRASE_PERMISSION), 0);
    assert_eq!(q.next(0), Some(st(PHRASE_PERMISSION)));
    q.push(answer("答え"), 10);
    assert_eq!(q.next(10), None);
    q.finished(20);
    assert_eq!(q.next(20), Some(st("答え")));
}

#[test]
fn queue_later_answer_while_speaking_replaces_the_waiting_one() {
    let mut q = SpeechQueue::new();
    q.push(answer("話し中"), 0);
    assert_eq!(q.next(0), Some(st("話し中")));
    q.push(answer("待ち一"), 1);
    q.push(answer("待ち二"), 2);
    q.finished(3);
    assert_eq!(q.next(3), Some(st("待ち二")));
    q.finished(4);
    assert_eq!(q.next(4), None);
}

#[test]
fn queue_answer_ignores_the_cooldown() {
    let mut q = SpeechQueue::new();
    q.push(answer("同じ答え"), 0);
    assert_eq!(q.next(0), Some(st("同じ答え")));
    q.finished(1);
    q.push(answer("同じ答え"), 2);
    assert_eq!(q.next(2), Some(st("同じ答え")));
    q.finished(3);

    // 固定文言と同じ文面の答えでも、冷却には掛からない。
    let mut q = queue_after_phrase(PHRASE_DONE, 0);
    q.push(answer(PHRASE_DONE), 100);
    assert_eq!(q.next(100), Some(st(PHRASE_DONE)));
}

#[test]
fn queue_speaking_an_answer_does_not_start_a_phrase_cooldown() {
    let mut q = SpeechQueue::new();
    q.push(answer(PHRASE_DONE), 0);
    assert_eq!(q.next(0), Some(st(PHRASE_DONE)));
    q.finished(1);
    q.push(phrase(PHRASE_DONE), 2);
    assert_eq!(q.next(2), Some(st(PHRASE_DONE)));
}

#[test]
fn queue_finished_releases_speaking_and_keeps_the_waiting_item() {
    let mut q = SpeechQueue::new();
    q.push(answer("話し中"), 0);
    assert_eq!(q.next(0), Some(st("話し中")));
    q.push(answer("待ち"), 1);
    q.finished(2);
    assert!(!q.speaking());
    assert_eq!(q.next(2), Some(st("待ち")));
}

#[test]
fn queue_finished_while_idle_is_a_no_op() {
    let mut q = SpeechQueue::new();
    q.finished(0);
    assert!(!q.speaking());
    q.push(phrase(PHRASE_DONE), 1);
    q.finished(2); // 話し中でないので待っている 1 件は残る
    assert!(!q.speaking());
    assert_eq!(q.next(2), Some(st(PHRASE_DONE)));
}

#[test]
fn queue_clear_drops_the_waiting_item_and_releases_speaking() {
    let mut q = SpeechQueue::new();
    q.push(answer("話し中"), 0);
    assert_eq!(q.next(0), Some(st("話し中")));
    q.push(answer("待ち"), 1);
    q.clear();
    assert!(!q.speaking());
    assert_eq!(q.next(2), None);
}

#[test]
fn queue_clear_drops_a_waiting_phrase_too() {
    let mut q = SpeechQueue::new();
    q.push(phrase(PHRASE_DONE), 0);
    q.clear();
    assert_eq!(q.next(1), None);
}

#[test]
fn queue_clear_keeps_the_phrase_cooldown() {
    let mut q = SpeechQueue::new();
    q.push(phrase(PHRASE_DONE), 0);
    assert_eq!(q.next(0), Some(st(PHRASE_DONE)));
    q.clear();
    assert!(!q.speaking());

    q.push(phrase(PHRASE_DONE), 14_999);
    assert_eq!(q.next(14_999), None);
    q.push(phrase(PHRASE_DONE), 15_000);
    assert_eq!(q.next(15_000), Some(st(PHRASE_DONE)));
}

#[test]
fn queue_clear_on_an_idle_queue_is_harmless() {
    let mut q = SpeechQueue::new();
    q.clear();
    assert!(!q.speaking());
    assert_eq!(q.next(0), None);
    q.push(answer("答え"), 1);
    assert_eq!(q.next(1), Some(st("答え")));
}

#[test]
fn queue_is_reusable_after_a_full_cycle() {
    let mut q = SpeechQueue::new();
    for (i, text) in ["一", "二", "三"].iter().enumerate() {
        let t = i as u64 * 10;
        q.push(answer(text), t);
        assert_eq!(q.next(t), Some(st(text)));
        assert!(q.speaking());
        q.finished(t + 5);
        assert!(!q.speaking());
    }
}

// ---------------------------------------------------------------------------
// 3. rms と Endpointer
// ---------------------------------------------------------------------------

/// +amp と -amp を交互に並べた n サンプル（RMS はちょうど amp）。
fn tone(amp: i16, n: usize) -> Vec<i16> {
    (0..n)
        .map(|i| if i % 2 == 0 { amp } else { -amp })
        .collect()
}

/// 30ms（`FRAME_SAMPLES`）の 1 フレーム。
fn frame(amp: i16) -> Vec<i16> {
    tone(amp, FRAME_SAMPLES)
}

/// amp の音を `ms` ミリ秒ぶん 10ms（160 サンプル）刻みで流し込み、各フレームの戻り値を返す。
fn feed(ep: &mut Endpointer, amp: i16, ms: u64) -> Vec<EndpointState> {
    assert_eq!(ms % 10, 0, "テストの書き間違い: ms は 10 の倍数");
    (0..ms / 10).map(|_| ep.push(&tone(amp, 160))).collect()
}

fn assert_listening(states: &[EndpointState]) {
    for (i, s) in states.iter().enumerate() {
        assert_eq!(
            *s,
            EndpointState::Listening,
            "{i} 番目のフレームで Listening でなくなった"
        );
    }
}

/// 無音で静かさを測り終えた直後（経過 300ms）の Endpointer。
fn calibrated() -> Endpointer {
    let mut ep = Endpointer::new();
    assert_listening(&feed(&mut ep, 0, CALIBRATION_MS));
    assert!(!ep.heard_speech());
    assert_eq!(ep.elapsed_ms(), CALIBRATION_MS);
    ep
}

#[test]
fn rms_of_empty_frame_is_zero() {
    assert_eq!(rms(&[]), 0.0);
}

#[test]
fn rms_of_known_frames() {
    assert_eq!(rms(&[0; 480]), 0.0);
    assert_close(rms(&tone(300, 480)), 300.0);
    assert_close(rms(&tone(1200, 160)), 1200.0);
    assert_close(rms(&[3, -4]), 12.5f32.sqrt());
    assert_close(rms(&[7]), 7.0);
    // 符号に依らない。
    assert_close(rms(&[-100]), rms(&[100]));
    // i16 の端でも溢れない。
    assert_close(rms(&[i16::MIN; 480]), 32768.0);
    assert_close(rms(&[i16::MAX; 480]), 32767.0);
}

#[test]
fn endpointer_starts_empty() {
    let ep = Endpointer::new();
    assert!(!ep.heard_speech());
    assert_eq!(ep.elapsed_ms(), 0);
}

#[test]
fn endpointer_elapsed_ms_counts_samples_and_floors_the_total() {
    let mut ep = Endpointer::new();
    ep.push(&frame(0));
    assert_eq!(ep.elapsed_ms(), 30);
    ep.push(&frame(0));
    assert_eq!(ep.elapsed_ms(), 60);

    // 16 サンプル＝1ms。合計してから切り捨てる（フレームごとに切り捨てない）。
    let mut ep = Endpointer::new();
    ep.push(&tone(0, 15));
    assert_eq!(ep.elapsed_ms(), 0);
    ep.push(&tone(0, 1));
    assert_eq!(ep.elapsed_ms(), 1);
    ep.push(&tone(0, 17));
    assert_eq!(ep.elapsed_ms(), 2); // 33 サンプル → 2.06ms
}

#[test]
fn endpointer_silence_only_ends_with_no_speech_at_5000ms() {
    let mut ep = Endpointer::new();
    assert_listening(&feed(&mut ep, 0, NO_SPEECH_MS - 10));
    assert_eq!(ep.elapsed_ms(), NO_SPEECH_MS - 10);
    assert_eq!(
        ep.push(&tone(0, 160)),
        EndpointState::Ended(EndReason::NoSpeech)
    );
    assert_eq!(ep.elapsed_ms(), NO_SPEECH_MS);
    assert!(!ep.heard_speech());
}

#[test]
fn endpointer_no_speech_boundary_counts_samples_not_frames() {
    let mut ep = Endpointer::new();
    for _ in 0..166 {
        // 166 × 30ms = 4980ms
        assert_eq!(ep.push(&frame(0)), EndpointState::Listening);
    }
    assert_eq!(ep.elapsed_ms(), 4_980);
    // あと 319 サンプルでは 4999ms（切り捨て）。
    assert_eq!(ep.push(&tone(0, 319)), EndpointState::Listening);
    assert_eq!(ep.elapsed_ms(), 4_999);
    // 1 サンプルで 5000ms ちょうど。
    assert_eq!(
        ep.push(&tone(0, 1)),
        EndpointState::Ended(EndReason::NoSpeech)
    );
    assert_eq!(ep.elapsed_ms(), 5_000);
}

#[test]
fn endpointer_silence_before_speech_never_ends_with_done() {
    // 話し始める前の沈黙が 1000ms を超えても Done にならない。
    let mut ep = Endpointer::new();
    assert_listening(&feed(&mut ep, 0, 4_000));
    assert!(!ep.heard_speech());
}

#[test]
fn endpointer_late_speech_is_heard_and_done_is_not_pre_empted_by_no_speech() {
    let mut ep = Endpointer::new();
    assert_listening(&feed(&mut ep, 0, 4_600));
    assert_listening(&feed(&mut ep, LOUD, MIN_SPEECH_MS)); // 経過 4900ms
    assert!(ep.heard_speech());
    // 経過が 5000ms を超えても、話した後は NoSpeech にならない。
    assert_listening(&feed(&mut ep, 0, END_SILENCE_MS - 10));
    assert_eq!(ep.elapsed_ms(), 4_900 + END_SILENCE_MS - 10);
    assert_eq!(
        ep.push(&tone(0, 160)),
        EndpointState::Ended(EndReason::Done)
    );
}

#[test]
fn endpointer_speech_then_silence_ends_with_done_at_1000ms() {
    let mut ep = calibrated();
    assert_listening(&feed(&mut ep, LOUD, MIN_SPEECH_MS));
    assert!(ep.heard_speech());
    assert_listening(&feed(&mut ep, 0, END_SILENCE_MS - 10)); // 990ms
    assert_eq!(
        ep.push(&tone(0, 160)),
        EndpointState::Ended(EndReason::Done)
    );
    assert!(ep.heard_speech());
    assert_eq!(
        ep.elapsed_ms(),
        CALIBRATION_MS + MIN_SPEECH_MS + END_SILENCE_MS
    );
}

#[test]
fn endpointer_done_with_30ms_frames_comes_on_the_34th_silent_frame() {
    let mut ep = Endpointer::new();
    for _ in 0..10 {
        // 静かさを測る 300ms
        assert_eq!(ep.push(&frame(0)), EndpointState::Listening);
    }
    for _ in 0..10 {
        // 話す 300ms
        assert_eq!(ep.push(&frame(LOUD)), EndpointState::Listening);
    }
    assert!(ep.heard_speech());
    for i in 1..=33 {
        // 33 × 30 = 990ms
        assert_eq!(
            ep.push(&frame(0)),
            EndpointState::Listening,
            "{i} 枚目の無音"
        );
    }
    assert_eq!(
        ep.push(&frame(0)), // 34 × 30 = 1020ms
        EndpointState::Ended(EndReason::Done)
    );
    assert_eq!(ep.elapsed_ms(), 10 * 30 + 10 * 30 + 34 * 30);
}

#[test]
fn endpointer_heard_speech_turns_true_exactly_at_min_speech_ms() {
    let mut ep = calibrated();
    assert_listening(&feed(&mut ep, LOUD, MIN_SPEECH_MS - 10)); // 290ms
    assert!(!ep.heard_speech());
    assert_listening(&feed(&mut ep, LOUD, 10)); // 300ms
    assert!(ep.heard_speech());
}

#[test]
fn endpointer_speech_below_min_speech_ms_never_ends_with_done() {
    let mut ep = calibrated();
    assert_listening(&feed(&mut ep, LOUD, MIN_SPEECH_MS - 10)); // 290ms
    assert!(!ep.heard_speech());
    // その後 1.5 秒黙っても Done にならない。
    assert_listening(&feed(&mut ep, 0, 1_500));
    assert!(!ep.heard_speech());
    assert_eq!(ep.elapsed_ms(), CALIBRATION_MS + 290 + 1_500);
    // 最後は NoSpeech で終わる（5000ms ちょうど）。
    assert_listening(&feed(&mut ep, 0, 2_900));
    assert_eq!(
        ep.push(&tone(0, 160)),
        EndpointState::Ended(EndReason::NoSpeech)
    );
    assert_eq!(ep.elapsed_ms(), NO_SPEECH_MS);
    assert!(!ep.heard_speech());
}

#[test]
fn endpointer_speech_length_is_summed_across_gaps() {
    let mut ep = calibrated();
    assert_listening(&feed(&mut ep, LOUD, 150));
    assert_listening(&feed(&mut ep, 0, 100));
    assert_listening(&feed(&mut ep, LOUD, 140)); // 合計 290ms
    assert!(!ep.heard_speech());
    assert_listening(&feed(&mut ep, LOUD, 10)); // 合計 300ms
    assert!(ep.heard_speech());
    assert_listening(&feed(&mut ep, 0, END_SILENCE_MS - 10));
    assert_eq!(
        ep.push(&tone(0, 160)),
        EndpointState::Ended(EndReason::Done)
    );
}

#[test]
fn endpointer_speech_resets_the_silence_counter() {
    let mut ep = calibrated();
    assert_listening(&feed(&mut ep, LOUD, MIN_SPEECH_MS));
    assert_listening(&feed(&mut ep, 0, END_SILENCE_MS - 10)); // 990ms
    assert_listening(&feed(&mut ep, LOUD, 10)); // 話して 0 に戻る
    assert_listening(&feed(&mut ep, 0, END_SILENCE_MS - 10)); // また 990ms
    assert_eq!(
        ep.push(&tone(0, 160)),
        EndpointState::Ended(EndReason::Done)
    );
}

#[test]
fn endpointer_continuous_speech_ends_with_max_length_at_15000ms() {
    let mut ep = calibrated();
    assert_listening(&feed(&mut ep, LOUD, MAX_RECORD_MS - CALIBRATION_MS - 10));
    assert_eq!(ep.elapsed_ms(), MAX_RECORD_MS - 10);
    assert!(ep.heard_speech());
    assert_eq!(
        ep.push(&tone(LOUD, 160)),
        EndpointState::Ended(EndReason::MaxLength)
    );
    assert_eq!(ep.elapsed_ms(), MAX_RECORD_MS);
    assert!(ep.heard_speech());
}

#[test]
fn endpointer_done_wins_over_max_length_when_both_hold_on_the_same_frame() {
    let mut ep = calibrated();
    assert_listening(&feed(&mut ep, LOUD, 13_700)); // 経過 14000ms
    assert_listening(&feed(&mut ep, 0, END_SILENCE_MS - 10)); // 14990ms
    assert_eq!(
        ep.push(&tone(0, 160)), // 15000ms かつ 黙りが 1000ms
        EndpointState::Ended(EndReason::Done)
    );
}

#[test]
fn endpointer_speech_from_the_start_is_not_hidden_by_the_quiet_estimate() {
    // 静かさは測る間の rms の「最小値」。最初の 1 フレームだけ静かで、残りの測る間も話している場合、
    // 平均で測るとしきい値が 13500 まで上がって話しているのに拾えなくなる。最小値なら 300。
    let mut ep = Endpointer::new();
    assert_eq!(ep.push(&frame(0)), EndpointState::Listening);
    for _ in 0..9 {
        assert_eq!(ep.push(&frame(LOUD)), EndpointState::Listening);
    }
    assert_eq!(ep.elapsed_ms(), CALIBRATION_MS);
    assert!(!ep.heard_speech(), "測る間のフレームは話しているかを見ない");
    for i in 1..=9 {
        assert_eq!(ep.push(&frame(LOUD)), EndpointState::Listening, "{i} 枚目");
    }
    assert!(!ep.heard_speech(), "話した長さは 270ms");
    assert_eq!(ep.push(&frame(LOUD)), EndpointState::Listening);
    assert!(ep.heard_speech(), "話した長さが 300ms になった");
}

#[test]
fn endpointer_calibration_window_includes_the_frame_ending_at_300ms() {
    // 経過 300ms ちょうどで受け終えるフレームは「測る間」（以下）。話しているとは数えない。
    let mut ep = Endpointer::new();
    for _ in 0..9 {
        assert_eq!(ep.push(&frame(0)), EndpointState::Listening);
    }
    assert_eq!(ep.push(&frame(LOUD)), EndpointState::Listening); // 270 → 300ms
    assert_eq!(ep.elapsed_ms(), 300);
    for i in 1..=9 {
        assert_eq!(ep.push(&frame(LOUD)), EndpointState::Listening, "{i} 枚目");
    }
    assert!(
        !ep.heard_speech(),
        "300ms ちょうどのフレームを話しているに数えると、ここで 300ms に届いてしまう"
    );
    assert_eq!(ep.push(&frame(LOUD)), EndpointState::Listening);
    assert!(ep.heard_speech());
}

#[test]
fn endpointer_threshold_is_max_of_floor_and_three_times_the_quiet_level() {
    // (測る間の背景音の振幅, その後に流す音の振幅, 話していると判定されるか)
    let cases: [(i16, i16, bool); 12] = [
        (0, 299, false),
        (0, 300, true),
        (50, 299, false),
        (50, 300, true),
        (100, 299, false),
        (100, 300, true), // 3×100 = 300 で、下限と同じ
        (200, 599, false),
        (200, 600, true), // 3×200 = 600
        (400, 1199, false),
        (400, 1200, true), // 3×400 = 1200
        (400, 300, false), // 下限は超えても、3×静かさに届かない
        (400, 5000, true),
    ];
    for (noise, sound, expect_heard) in cases {
        let mut ep = Endpointer::new();
        assert_listening(&feed(&mut ep, noise, CALIBRATION_MS));
        assert_listening(&feed(&mut ep, sound, 600));
        assert_eq!(
            ep.heard_speech(),
            expect_heard,
            "背景 {noise} / 音 {sound} で heard_speech が食い違う"
        );
    }
}

#[test]
fn endpointer_sound_below_speech_rms_min_is_not_speech() {
    let mut ep = calibrated();
    assert_listening(&feed(&mut ep, 299, NO_SPEECH_MS - CALIBRATION_MS - 10));
    assert!(!ep.heard_speech());
    assert_eq!(
        ep.push(&tone(299, 160)),
        EndpointState::Ended(EndReason::NoSpeech)
    );
    assert!(!ep.heard_speech());
}

/// 3 通りの終わり方をした Endpointer（終わり方つき）。
fn ended_endpointers() -> Vec<(Endpointer, EndReason)> {
    let mut no_speech = Endpointer::new();
    assert_listening(&feed(&mut no_speech, 0, NO_SPEECH_MS - 10));
    assert_eq!(
        no_speech.push(&tone(0, 160)),
        EndpointState::Ended(EndReason::NoSpeech)
    );

    let mut done = calibrated();
    assert_listening(&feed(&mut done, LOUD, MIN_SPEECH_MS));
    assert_listening(&feed(&mut done, 0, END_SILENCE_MS - 10));
    assert_eq!(
        done.push(&tone(0, 160)),
        EndpointState::Ended(EndReason::Done)
    );

    let mut max_length = calibrated();
    assert_listening(&feed(
        &mut max_length,
        LOUD,
        MAX_RECORD_MS - CALIBRATION_MS - 10,
    ));
    assert_eq!(
        max_length.push(&tone(LOUD, 160)),
        EndpointState::Ended(EndReason::MaxLength)
    );

    vec![
        (no_speech, EndReason::NoSpeech),
        (done, EndReason::Done),
        (max_length, EndReason::MaxLength),
    ]
}

#[test]
fn endpointer_ended_state_never_changes() {
    for (mut ep, reason) in ended_endpointers() {
        let elapsed = ep.elapsed_ms();
        let heard = ep.heard_speech();
        for _ in 0..20 {
            assert_eq!(ep.push(&frame(LOUD)), EndpointState::Ended(reason));
            assert_eq!(ep.push(&frame(0)), EndpointState::Ended(reason));
            assert_eq!(ep.push(&tone(LOUD, 7)), EndpointState::Ended(reason));
            assert_eq!(ep.push(&[]), EndpointState::Ended(reason));
        }
        assert_eq!(
            ep.elapsed_ms(),
            elapsed,
            "{reason:?} の後に経過時間が進んだ"
        );
        assert_eq!(
            ep.heard_speech(),
            heard,
            "{reason:?} の後に heard_speech が変わった"
        );
    }
}

#[test]
fn endpointer_empty_frame_at_the_start_changes_nothing() {
    let mut ep = Endpointer::new();
    assert_eq!(ep.push(&[]), EndpointState::Listening);
    assert_eq!(ep.elapsed_ms(), 0);
    assert!(!ep.heard_speech());
}

#[test]
fn endpointer_empty_frame_does_not_advance_time_or_the_silence_counter() {
    let mut ep = calibrated();
    assert_listening(&feed(&mut ep, LOUD, MIN_SPEECH_MS));
    assert_listening(&feed(&mut ep, 0, END_SILENCE_MS - 10)); // 990ms
    let elapsed = ep.elapsed_ms();
    for _ in 0..5 {
        assert_eq!(ep.push(&[]), EndpointState::Listening);
    }
    assert_eq!(ep.elapsed_ms(), elapsed);
    assert!(ep.heard_speech());
    // 空のフレームを挟んでも、次の 10ms で 1000ms ちょうど。
    assert_eq!(
        ep.push(&tone(0, 160)),
        EndpointState::Ended(EndReason::Done)
    );
}

#[test]
fn endpointer_empty_frame_is_not_counted_as_a_quiet_sample() {
    // 背景音 400 → しきい値 1200。空のフレーム（rms 0.0）が静かさに混ざると 0 になり、
    // しきい値 300 で 1000 の音を話していると取り違える。
    let mut ep = Endpointer::new();
    assert_eq!(ep.push(&[]), EndpointState::Listening);
    assert_listening(&feed(&mut ep, 400, CALIBRATION_MS));
    assert_eq!(ep.elapsed_ms(), CALIBRATION_MS);
    assert_listening(&feed(&mut ep, 1_000, 600));
    assert!(!ep.heard_speech());
}

// ---------------------------------------------------------------------------
// 4. transcript_text
// ---------------------------------------------------------------------------

#[test]
fn transcript_plain_sentence_is_passed_through() {
    assert_eq!(
        transcript_text("今日の天気を教えて"),
        Some(st("今日の天気を教えて"))
    );
    // 句読点は消さない（key を作る時にだけ除く）。
    assert_eq!(
        transcript_text("ねえ、今日は何する？"),
        Some(st("ねえ、今日は何する？"))
    );
    assert_eq!(
        transcript_text("Please run cargo test."),
        Some(st("Please run cargo test."))
    );
}

#[test]
fn transcript_whitespace_is_normalized_to_one_line() {
    assert_eq!(
        transcript_text("  こんにちは \n  世界 \t"),
        Some(st("こんにちは 世界"))
    );
}

#[test]
fn transcript_hallucinations_are_dropped_verbatim() {
    for h in HALLUCINATIONS {
        assert_eq!(transcript_text(h), None, "{h:?}");
    }
}

#[test]
fn transcript_hallucinations_are_dropped_with_punctuation_and_spaces() {
    for h in HALLUCINATIONS {
        let spaced_ascii = h.chars().map(String::from).collect::<Vec<_>>().join(" ");
        let spaced_wide = h
            .chars()
            .map(String::from)
            .collect::<Vec<_>>()
            .join("\u{3000}");
        let commas = h.chars().map(String::from).collect::<Vec<_>>().join("、");
        let variants = [
            format!("{h}。"),
            format!("{h}！"),
            format!("{h}!!"),
            format!("{h}…"),
            format!("{h}..."),
            format!("。{h}"),
            format!("「{h}」"),
            format!("（{h}）"),
            format!("({h})"),
            format!("『{h}』"),
            format!("  {h}  "),
            format!("\n{h}\n"),
            format!("\t{h}。\n"),
            format!("{h}？"),
            format!("{h}・"),
            format!("{h}、"),
            format!("{h}．"),
            format!("{h}，"),
            spaced_ascii,
            spaced_wide,
            commas,
        ];
        for v in variants {
            assert_eq!(transcript_text(&v), None, "{v:?}");
        }
    }
}

#[test]
fn transcript_only_exact_hallucination_match_is_dropped() {
    for h in HALLUCINATIONS {
        for v in [
            format!("{h}と言いました"),
            format!("えーと{h}"),
            format!("{h}か"),
            format!("こんにちは。{h}"),
        ] {
            assert_eq!(
                transcript_text(&v),
                Some(v.clone()),
                "部分一致で捨ててはいけない: {v:?}"
            );
        }
    }
}

#[test]
fn transcript_punctuation_or_whitespace_only_is_dropped() {
    for raw in [
        "",
        " ",
        "   ",
        "\n\t\r\n",
        "\u{3000}",
        "。",
        "。。。",
        "…",
        "、，．.,!！?？・…「」『』（）()",
        "  ・・・  ",
        "（　）",
        "「」",
    ] {
        assert_eq!(transcript_text(raw), None, "{raw:?}");
    }
}

#[test]
fn transcript_2000_chars_is_sent_and_2001_is_not() {
    let ok = cycle("あいうえお", 2000);
    assert_eq!(transcript_text(&ok), Some(ok.clone()));
    // 前後の空白は文字数に数えない。
    assert_eq!(transcript_text(&format!("  {ok}\n")), Some(ok.clone()));

    let too_long = cycle("あいうえお", 2001);
    assert_eq!(transcript_text(&too_long), None);
    assert_eq!(transcript_text(&cycle("あいうえお", 5000)), None);
}

// ---------------------------------------------------------------------------
// 5. wav_bytes と parse_wav
// ---------------------------------------------------------------------------

fn fmt_body(format: u16, channels: u16, rate: u32, bits: u16) -> Vec<u8> {
    let block = channels * bits / 8;
    let mut b = Vec::new();
    b.extend_from_slice(&format.to_le_bytes());
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * block as u32).to_le_bytes());
    b.extend_from_slice(&block.to_le_bytes());
    b.extend_from_slice(&bits.to_le_bytes());
    b
}

/// チャンク 1 つ（id・長さ・中身。長さが奇数なら 1 バイトの詰め物）。
fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut c = raw_chunk(id, body.len() as u32, body);
    if body.len() % 2 == 1 {
        c.push(0);
    }
    c
}

/// 宣言する長さと中身を別々に指定するチャンク（詰め物なし）。壊れたファイルを作る用。
fn raw_chunk(id: &[u8; 4], declared_len: u32, body: &[u8]) -> Vec<u8> {
    let mut c = Vec::new();
    c.extend_from_slice(id);
    c.extend_from_slice(&declared_len.to_le_bytes());
    c.extend_from_slice(body);
    c
}

/// RIFF/WAVE の頭（サイズは実際に合わせる）に、チャンクを順に並べる。
fn riff(chunks: &[Vec<u8>]) -> Vec<u8> {
    let rest: Vec<u8> = chunks.concat();
    let mut w = Vec::new();
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(4 + rest.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVE");
    w.extend_from_slice(&rest);
    w
}

/// 16 ビット PCM の中身（n サンプル分）。
fn pcm_body(n: usize) -> Vec<u8> {
    tone(1000, n).iter().flat_map(|s| s.to_le_bytes()).collect()
}

fn good_fmt() -> Vec<u8> {
    chunk(b"fmt ", &fmt_body(1, 1, 16_000, 16))
}

fn good_data(n: usize) -> Vec<u8> {
    chunk(b"data", &pcm_body(n))
}

fn info(channels: u16, sample_rate: u32, data_len: usize) -> WavInfo {
    WavInfo {
        channels,
        sample_rate,
        bits_per_sample: 16,
        data_len,
    }
}

#[test]
fn wav_bytes_layout_is_a_44_byte_pcm_header_plus_samples() {
    let samples = [1i16, -2, 0x0102];
    let w = wav_bytes(&samples, 16_000);

    let mut want = Vec::new();
    want.extend_from_slice(b"RIFF");
    want.extend_from_slice(&42u32.to_le_bytes()); // 36 + 6
    want.extend_from_slice(b"WAVE");
    want.extend_from_slice(b"fmt ");
    want.extend_from_slice(&16u32.to_le_bytes());
    want.extend_from_slice(&1u16.to_le_bytes()); // PCM
    want.extend_from_slice(&1u16.to_le_bytes()); // モノラル
    want.extend_from_slice(&16_000u32.to_le_bytes());
    want.extend_from_slice(&32_000u32.to_le_bytes()); // 毎秒バイト
    want.extend_from_slice(&2u16.to_le_bytes()); // ブロック
    want.extend_from_slice(&16u16.to_le_bytes()); // ビット
    want.extend_from_slice(b"data");
    want.extend_from_slice(&6u32.to_le_bytes());
    want.extend_from_slice(&1i16.to_le_bytes());
    want.extend_from_slice(&(-2i16).to_le_bytes());
    want.extend_from_slice(&0x0102i16.to_le_bytes());

    assert_eq!(w.len(), 50);
    assert_eq!(w, want);
}

#[test]
fn wav_bytes_header_follows_the_sample_rate_and_length() {
    let w = wav_bytes(&tone(500, 100), 44_100);
    assert_eq!(w.len(), 44 + 200);
    assert_eq!(&w[0..4], b"RIFF");
    assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()), 36 + 200);
    assert_eq!(&w[8..12], b"WAVE");
    assert_eq!(&w[12..16], b"fmt ");
    assert_eq!(u32::from_le_bytes(w[16..20].try_into().unwrap()), 16);
    assert_eq!(u16::from_le_bytes(w[20..22].try_into().unwrap()), 1);
    assert_eq!(u16::from_le_bytes(w[22..24].try_into().unwrap()), 1);
    assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 44_100);
    assert_eq!(u32::from_le_bytes(w[28..32].try_into().unwrap()), 88_200);
    assert_eq!(u16::from_le_bytes(w[32..34].try_into().unwrap()), 2);
    assert_eq!(u16::from_le_bytes(w[34..36].try_into().unwrap()), 16);
    assert_eq!(&w[36..40], b"data");
    assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 200);
}

#[test]
fn wav_bytes_of_no_samples_is_only_the_header_and_parse_rejects_it() {
    let w = wav_bytes(&[], 16_000);
    assert_eq!(w.len(), 44);
    assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()), 36);
    assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 0);
    // data の長さが 0 のファイルは読めない扱い。
    assert_eq!(parse_wav(&w), None);
}

#[test]
fn parse_wav_reads_what_wav_bytes_wrote() {
    for (n, rate) in [
        (1, 16_000),
        (3, 8_000),
        (480, 16_000),
        (100, 44_100),
        (8_000, 24_000),
    ] {
        let w = wav_bytes(&tone(1234, n), rate);
        assert_eq!(
            parse_wav(&w),
            Some(info(1, rate, n * 2)),
            "{n} サンプル / {rate}Hz"
        );
    }
}

#[test]
fn parse_wav_rejects_every_strict_prefix_of_a_valid_file() {
    let w = wav_bytes(&tone(1000, 50), 16_000);
    assert_eq!(parse_wav(&w), Some(info(1, 16_000, 100)));
    for cut in 0..w.len() {
        assert_eq!(
            parse_wav(&w[..cut]),
            None,
            "先頭 {cut} バイトだけでは読めない"
        );
    }
}

#[test]
fn parse_wav_rejects_a_broken_header() {
    let good = wav_bytes(&tone(1000, 20), 16_000);

    let mut not_riff = good.clone();
    not_riff[0] = b'X'; // RIFX
    let mut not_wave = good.clone();
    not_wave[8..12].copy_from_slice(b"WAVX");
    let mut zeroed = good.clone();
    zeroed[0..12].fill(0);

    assert_eq!(parse_wav(&not_riff), None);
    assert_eq!(parse_wav(&not_wave), None);
    assert_eq!(parse_wav(&zeroed), None);
    assert_eq!(parse_wav(&[]), None);
    assert_eq!(parse_wav(b"RIFF"), None);
    assert_eq!(parse_wav(b"RIFF\0\0\0\0WAV"), None); // 11 バイト
    assert_eq!(parse_wav(b"RIFF\0\0\0\0WAVE"), None); // 頭だけでチャンクが無い
    assert_eq!(parse_wav(&[0xFF; 100]), None);
    assert_eq!(parse_wav(b"not a wav file at all, just some text"), None);
}

#[test]
fn parse_wav_ignores_the_riff_size_field() {
    let good = wav_bytes(&tone(1000, 20), 16_000);
    for size in [0u32, 1, 12345, u32::MAX] {
        let mut w = good.clone();
        w[4..8].copy_from_slice(&size.to_le_bytes());
        assert_eq!(
            parse_wav(&w),
            Some(info(1, 16_000, 40)),
            "RIFF サイズ {size}"
        );
    }
}

#[test]
fn parse_wav_rejects_data_that_does_not_fit_in_the_rest() {
    let good = wav_bytes(&tone(1000, 10), 16_000); // data は 20 バイト
    assert!(parse_wav(&good).is_some());

    // 1 バイト足りない。
    assert_eq!(parse_wav(&good[..good.len() - 1]), None);
    // 宣言が実際より長い。
    for declared in [21u32, 100, 1 << 20, u32::MAX] {
        let mut w = good.clone();
        w[40..44].copy_from_slice(&declared.to_le_bytes());
        assert_eq!(parse_wav(&w), None, "宣言 {declared} バイト");
    }
    // 宣言が 0（中身はある）。
    let mut zero = good.clone();
    zero[40..44].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(parse_wav(&zero), None);
}

#[test]
fn parse_wav_reports_the_declared_data_len_and_ignores_trailing_bytes() {
    let mut w = wav_bytes(&tone(1000, 10), 16_000);
    w.extend_from_slice(&[1, 2, 3]); // data の後ろの余り
    assert_eq!(parse_wav(&w), Some(info(1, 16_000, 20)));

    // data の後ろが壊れたチャンクでも、data を見つけた時点で読むのをやめる。
    let mut w = wav_bytes(&tone(1000, 10), 16_000);
    w.extend_from_slice(&raw_chunk(b"LIST", 9999, b"xx"));
    assert_eq!(parse_wav(&w), Some(info(1, 16_000, 20)));
}

#[test]
fn parse_wav_skips_a_list_chunk_between_fmt_and_data() {
    let w = riff(&[good_fmt(), chunk(b"LIST", b"INFOabcd"), good_data(10)]);
    assert_eq!(parse_wav(&w), Some(info(1, 16_000, 20)));
}

#[test]
fn parse_wav_skips_unknown_chunks_before_fmt_and_between() {
    let w = riff(&[
        chunk(b"JUNK", &[0; 28]),
        good_fmt(),
        chunk(b"fact", &[0; 4]),
        chunk(b"LIST", b"INFOabcd"),
        good_data(10),
    ]);
    assert_eq!(parse_wav(&w), Some(info(1, 16_000, 20)));
}

#[test]
fn parse_wav_honours_the_pad_byte_after_an_odd_length_chunk() {
    // 長さ 5 の LIST は後ろに 1 バイトの詰め物がある。詰め物を数えないと次のチャンクの頭がずれる。
    let odd = chunk(b"LIST", b"12345");
    assert_eq!(odd.len(), 8 + 5 + 1);
    let w = riff(&[good_fmt(), odd, good_data(10)]);
    assert_eq!(parse_wav(&w), Some(info(1, 16_000, 20)));

    // fmt の前でも同じ。
    let w = riff(&[chunk(b"JUNK", b"abc"), good_fmt(), good_data(10)]);
    assert_eq!(parse_wav(&w), Some(info(1, 16_000, 20)));
}

#[test]
fn parse_wav_accepts_an_odd_length_data_chunk_without_padding() {
    let w = riff(&[good_fmt(), raw_chunk(b"data", 3, &[1, 2, 3])]);
    assert_eq!(parse_wav(&w), Some(info(1, 16_000, 3)));
}

#[test]
fn parse_wav_rejects_an_unknown_chunk_that_overruns_the_file() {
    let w = riff(&[good_fmt(), raw_chunk(b"LIST", 1000, b"INFO"), good_data(10)]);
    // 1000 バイト進むと data の頭も越えるので、data が見つからない。
    assert_eq!(parse_wav(&w), None);

    let w = riff(&[good_fmt(), raw_chunk(b"LIST", 8, b"INFO")]);
    assert_eq!(parse_wav(&w), None);
}

#[test]
fn parse_wav_stops_quietly_when_a_chunk_header_is_cut_short() {
    // fmt の後に 8 バイトに満たない残り。data が無いまま終わるので None。
    let mut w = riff(&[good_fmt()]);
    w.extend_from_slice(&[1, 2, 3, 4, 5]);
    assert_eq!(parse_wav(&w), None);

    let mut w = riff(&[good_fmt()]);
    w.extend_from_slice(b"dat");
    assert_eq!(parse_wav(&w), None);
}

#[test]
fn parse_wav_rejects_data_before_fmt() {
    let w = riff(&[good_data(10), good_fmt()]);
    assert_eq!(parse_wav(&w), None);

    let w = riff(&[chunk(b"LIST", b"INFO"), good_data(10), good_fmt()]);
    assert_eq!(parse_wav(&w), None);
}

#[test]
fn parse_wav_rejects_a_file_without_fmt_or_without_data() {
    assert_eq!(parse_wav(&riff(&[good_data(10)])), None);
    assert_eq!(parse_wav(&riff(&[good_fmt()])), None);
    assert_eq!(parse_wav(&riff(&[])), None);
    assert_eq!(
        parse_wav(&riff(&[good_fmt(), chunk(b"LIST", b"INFOabcd")])),
        None
    );
    assert_eq!(parse_wav(&riff(&[chunk(b"LIST", b"INFOabcd")])), None);
}

#[test]
fn parse_wav_accepts_only_16bit_pcm_mono_or_stereo() {
    // (形式, チャンネル, ビット, 読めるか)
    let cases: [(u16, u16, u16, bool); 14] = [
        (1, 1, 16, true),
        (1, 2, 16, true),
        (3, 1, 16, false),
        (3, 1, 32, false), // IEEE float
        (3, 2, 32, false),
        (1, 1, 8, false),
        (1, 1, 24, false),
        (1, 1, 32, false),
        (1, 0, 16, false),
        (1, 3, 16, false),
        (1, 6, 16, false),
        (0, 1, 16, false),
        (2, 1, 16, false), // ADPCM
        (0xFFFE, 1, 16, false),
    ];
    for (format, channels, bits, ok) in cases {
        let w = riff(&[
            chunk(b"fmt ", &fmt_body(format, channels, 16_000, bits)),
            good_data(10),
        ]);
        let want = if ok {
            Some(WavInfo {
                channels,
                sample_rate: 16_000,
                bits_per_sample: bits,
                data_len: 20,
            })
        } else {
            None
        };
        assert_eq!(
            parse_wav(&w),
            want,
            "形式 {format} / {channels}ch / {bits}bit"
        );
    }
}

#[test]
fn parse_wav_reports_the_sample_rate_in_the_fmt_chunk() {
    for rate in [8_000u32, 16_000, 22_050, 44_100, 48_000] {
        let w = riff(&[chunk(b"fmt ", &fmt_body(1, 1, rate, 16)), good_data(4)]);
        assert_eq!(parse_wav(&w), Some(info(1, rate, 8)), "{rate}Hz");
    }
}

#[test]
fn parse_wav_checks_the_fmt_chunk_length() {
    let full = fmt_body(1, 1, 16_000, 16);
    assert_eq!(full.len(), 16);

    // 16 以上なら読める（拡張部分は読み飛ばす。奇数長は詰め物つき）。
    for extra in [0usize, 1, 2, 24] {
        let mut body = full.clone();
        body.extend(std::iter::repeat_n(0u8, extra));
        let w = riff(&[chunk(b"fmt ", &body), good_data(10)]);
        assert_eq!(
            parse_wav(&w),
            Some(info(1, 16_000, 20)),
            "fmt の長さ {}",
            16 + extra
        );
    }

    // 16 に満たない fmt は読めない。
    for len in [0usize, 2, 14, 15] {
        let w = riff(&[chunk(b"fmt ", &full[..len]), good_data(10)]);
        assert_eq!(parse_wav(&w), None, "fmt の長さ {len}");
    }
}
