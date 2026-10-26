//! `pet_core::bubble`（吹き出しの固定文言の島の大きさ）の単体テスト。テスト先行。
//! 2026-10-27 K の報告「マイクを使えないよ（ミュートか Windows の…」が 180×40 のカプセルで切れた、から足した。
//! 字幅の根拠は Edge で 13px の本文を測った値（.steering/20261027-fix-bubble-size/brief.md）。

use pet_core::bubble::{
    STATUS_LINE_HEIGHT, STATUS_MAX_WIDTH, STATUS_MIN_WIDTH, STATUS_PAD_X, STATUS_PAD_Y, char_width,
    status_island, text_width, wrap,
};

const K_TEXT: &str = "マイクを使えないよ（ミュートか Windows の設定）";

fn content_width() -> f64 {
    STATUS_MAX_WIDTH - STATUS_PAD_X * 2.0
}

// ---- 字幅 ----

#[test]
fn wide_chars_are_wider_than_ascii() {
    // 全角は 13px の本文で 13.08px（実測）。半角より広い
    assert!(char_width('あ') > char_width('a'));
    assert!(char_width('漢') > char_width('W'));
    assert_eq!(char_width('あ'), char_width('）'));
}

#[test]
fn uppercase_and_m_w_are_wider_than_other_lowercase() {
    // 「Windows」が 55.3px（7.9px/字）で、小文字の平均 6.3px より広い。大文字と「m」「w」を別に数える
    assert!(char_width('W') > char_width('a'));
    assert!(char_width('M') > char_width('n'));
    assert!(char_width('w') > char_width('a'));
    assert!(char_width('m') > char_width('n'));
}

#[test]
fn control_and_newline_have_no_width() {
    assert_eq!(char_width('\n'), 0.0);
    assert_eq!(char_width('\u{200b}'), 0.0);
}

#[test]
fn estimated_width_is_at_least_the_measured_width() {
    // Edge の実測（2026-10-27・13px・DESIGN.md のフォント列）。見積もりは常に実測以上（切れない側に外す）
    let measured = [
        (K_TEXT, 307.6),
        ("Windows", 55.3),
        ("WINDOWS MICROPHONE", 150.0),
        ("the quick brown fox jumps over", 189.3),
        ("https://github.com/cytrynan/pet-claude/pull/36", 290.3),
        ("0123456789", 72.2),
        ("マイクを使えないよミュートか設定", 207.5),
    ];
    for (s, px) in measured {
        assert!(text_width(s) >= px, "{s}: {} < {px}", text_width(s));
        // 2 割以上は余らせない（幅が無駄に広がる）
        assert!(
            text_width(s) <= px * 1.25,
            "{s}: {} > {}",
            text_width(s),
            px * 1.25
        );
    }
}

// ---- 折り返し ----

#[test]
fn empty_text_is_one_line() {
    let w = wrap("", content_width());
    assert_eq!(w.lines, 1);
    assert_eq!(w.widest, 0.0);
}

#[test]
fn short_text_stays_on_one_line() {
    let w = wrap("できたよ", content_width());
    assert_eq!(w.lines, 1);
    assert_eq!(w.widest, text_width("できたよ"));
}

#[test]
fn k_text_wraps_to_two_lines() {
    // 307.6px は 288px に収まらない → 2 行
    let w = wrap(K_TEXT, content_width());
    assert_eq!(w.lines, 2);
    assert!(w.widest <= content_width());
}

#[test]
fn newline_starts_a_new_line_even_when_short() {
    let w = wrap("a\nb", content_width());
    assert_eq!(w.lines, 2);
    assert_eq!(w.widest, char_width('a'));
    // 空行も 1 行として数える（pre-wrap と同じ）
    assert_eq!(wrap("a\n\nb", content_width()).lines, 3);
    // 末尾の改行は空行を作らない（pre-wrap と同じ。末尾の改行で行が増えない）
    assert_eq!(wrap("a\n", content_width()).lines, 1);
}

#[test]
fn cjk_breaks_between_any_characters() {
    // 全角 30 字＝405px → 288px で 2 行（1 行目に 21 字）
    let w = wrap(&"あ".repeat(30), content_width());
    assert_eq!(w.lines, 2);
    assert!(w.widest <= content_width());
    assert!(w.widest > content_width() - char_width('あ'));
}

#[test]
fn latin_words_move_to_the_next_line_whole() {
    // 「aaaa bbbb」を 1 語しか入らない幅で折ると、語の途中で切らず 2 行になる
    let word = text_width("aaaa");
    let w = wrap("aaaa bbbb", word + char_width(' ') * 0.5);
    assert_eq!(w.lines, 2);
    assert_eq!(w.widest, word);
}

#[test]
fn a_word_longer_than_the_line_breaks_anywhere() {
    // URL のような英数字の連なりは overflow-wrap: anywhere で途中で折れる。100 字 ×7px＝700px → 288px で 3 行
    let url = "a".repeat(100);
    let w = wrap(&url, content_width());
    assert_eq!(w.lines, 3);
    assert!(w.widest <= content_width());
}

#[test]
fn real_url_in_a_sentence_wraps_without_overflow() {
    // Edge の実測（2026-10-27）: 「詳しくは https://github.com/cytrynan/pet-」「claude/blob/main/docs/architecture.md#63-音」
    // 「声入力 を見てね」の 3 行。ハイフンの後ろで折れ、「/」では折れない
    let s = "詳しくは https://github.com/cytrynan/pet-claude/blob/main/docs/architecture.md#63-音声入力 を見てね";
    let w = wrap(s, content_width());
    assert_eq!(w.lines, 3);
    assert!(w.widest <= content_width());
}

#[test]
fn a_long_word_moves_to_the_next_line_before_breaking_inside() {
    // Edge の実測: 「詳しくは 」＋ a×80 は、1 行目が「詳しくは 」だけになり、2 行目から語を折る（行の残りを埋めない）
    let w = wrap(&format!("詳しくは {}", "a".repeat(80)), content_width());
    let per_line = (content_width() / char_width('a')).floor() as usize;
    assert_eq!(w.lines, 1 + 80usize.div_ceil(per_line));
}

#[test]
fn hyphen_allows_a_break_after_it_but_not_before_a_digit() {
    // 「あ aaaa-」は行に入り「bbbb」が入らない幅: ハイフンの後ろで折れて 1 行目は「あ aaaa-」
    let max = text_width("aaaa-bbbb");
    let w = wrap("あ aaaa-bbbb", max);
    assert_eq!(w.lines, 2);
    assert_eq!(w.widest, text_width("あ aaaa-"));
    // 次が数字なら折らない（「-5」のような負号）: 語ごと 2 行目へ
    let max = text_width("123-456");
    let w = wrap("あ 123-456", max);
    assert_eq!(w.lines, 2);
    assert_eq!(w.widest, text_width("123-456"));
}

#[test]
fn slash_and_dot_do_not_allow_a_break() {
    // 「aaaa/bbbb」「aaaa.bbbb」は 1 つの語: 単独なら行に入るので丸ごと 2 行目へ
    for s in ["aaaa/bbbb", "aaaa.bbbb", "aaaa_bbbb"] {
        let w = wrap(&format!("あ {s}"), text_width(s));
        assert_eq!(w.lines, 2, "{s}");
        assert_eq!(w.widest, text_width(s), "{s}");
    }
}

#[test]
fn trailing_space_at_a_line_end_is_not_counted() {
    // 語の後の空白は行末では幅に入らない（ブラウザは行末の空白をぶら下げる）
    let word = text_width("aaaa");
    let w = wrap("aaaa bbbb", word + char_width(' ') * 0.5);
    assert_eq!(w.widest, word);
}

// ---- 島の大きさ ----

#[test]
fn short_status_keeps_the_default_capsule() {
    // 「できたよ」は従来どおり 180×40（見た目を変えない）
    assert_eq!(status_island("できたよ"), (STATUS_MIN_WIDTH, 40.0));
    assert_eq!(status_island(""), (STATUS_MIN_WIDTH, 40.0));
}

#[test]
fn one_line_status_widens_to_fit_the_text() {
    // 1 行に収まる長めの文は、文字の幅＋左右の余白まで広げる（上限まで）
    let s = "音声入力の準備が要るよ（設定を見てね）";
    let (w, h) = status_island(s);
    assert_eq!(h, 40.0);
    assert_eq!(w, (text_width(s) + STATUS_PAD_X * 2.0).ceil());
    assert!(w > STATUS_MIN_WIDTH && w <= STATUS_MAX_WIDTH);
}

#[test]
fn k_text_takes_full_width_and_two_lines() {
    let (w, h) = status_island(K_TEXT);
    assert_eq!(w, STATUS_MAX_WIDTH);
    assert_eq!(h, STATUS_PAD_Y * 2.0 + STATUS_LINE_HEIGHT * 2.0);
}

#[test]
fn height_grows_one_line_at_a_time_without_a_cap() {
    // 上限を超える文は省略せず、折り返して高さで取る（DESIGN.md・K 指示 2026-10-27）
    let long = "あ".repeat(200);
    let (w, h) = status_island(&long);
    assert_eq!(w, STATUS_MAX_WIDTH);
    let lines = wrap(&long, STATUS_MAX_WIDTH - STATUS_PAD_X * 2.0).lines;
    assert!(lines >= 10);
    assert_eq!(h, STATUS_PAD_Y * 2.0 + STATUS_LINE_HEIGHT * lines as f64);
}

#[test]
fn newlines_in_a_status_are_kept() {
    let (w, h) = status_island("できたよ\nほかに 2 件");
    assert_eq!(w, STATUS_MIN_WIDTH);
    assert_eq!(h, STATUS_PAD_Y * 2.0 + STATUS_LINE_HEIGHT * 2.0);
}

#[test]
fn sizes_are_whole_logical_pixels() {
    for s in ["", K_TEXT, "Windows", &"a".repeat(100)] {
        let (w, h) = status_island(s);
        assert_eq!(w, w.round());
        assert_eq!(h, h.round());
    }
}
