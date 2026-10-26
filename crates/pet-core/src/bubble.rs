//! 吹き出しの固定文言の島の大きさ（文字を数えて決める純粋な関数）。
//!
//! 2026-10-27 K の報告: 「マイクを使えないよ（ミュートか Windows の設定）」が 180×40 の
//! カプセルに収まらず切れた。文字の幅を見積もり、幅は上限まで広げ、収まらない分は折り返して
//! 高さで取る（省略しない。K 指示「文字数に応じて大きさを調節し、すべての文字が読める状態に」）。
//!
//! 字幅は Edge（WebView2 と同じエンジン）で DESIGN.md のフォント列・13px を測った値から決めた
//! （.steering/20261027-fix-bubble-size/brief.md）。見積もりは**常に実測より広い側**に外す
//! （狭く外すと切れる。広く外すと余白が増えるだけ）。それでも外れた分は bubble.html が描画後に
//! 測って `bubble:fit` で伝え、ui.rs が窓を広げる。

/// 全角 1 字の幅。実測 13.08px（13px の本文）。288px の行に実際は 22 字入るが、禁則（行頭の「、」「）」
/// などが前の字を道連れにする）で 1 字早く折れる余地として、21 字で数える 13.25
const WIDE_PX: f64 = 13.25;
/// 英大文字。実測の平均 8.3px だが「W」「M」は 12px 近い。語の先頭に 1 つ入る形が多いので 10
const UPPER_PX: f64 = 10.0;
/// 「m」「w」。小文字の中で飛び抜けて広い（「Windows」が 55.3px＝7.9px/字になる理由）
const WIDE_LOWER_PX: f64 = 10.5;
/// 数字。実測 7.22px
const DIGIT_PX: f64 = 7.5;
/// その他の半角（英小文字・記号・半角カナ）。実測の平均 6.3px。広く外しすぎると行数が 1 行多くなり
/// 下に空行が出る（7.5 で URL の文に出た）ので 7.0
const NARROW_PX: f64 = 7.0;

/// 固定文言の島: 幅は 180（「できたよ」の従来の見た目）〜320（承認のカードと同じ。DESIGN.md）。
/// 320 で止めるのは、ペット（最大でも 300px 程度）より大きく横へ広がると吹き出しに見えないため。
pub const STATUS_MIN_WIDTH: f64 = 180.0;
pub const STATUS_MAX_WIDTH: f64 = 320.0;
/// 左右の余白（カプセルの `padding: 0 16px`）と、上下の余白・1 行の高さ（本文 13px・行の高さ 20px）。
/// 1 行なら 10+20+10＝40 で従来のカプセルと同じ高さ
pub const STATUS_PAD_X: f64 = 16.0;
pub const STATUS_PAD_Y: f64 = 10.0;
pub const STATUS_LINE_HEIGHT: f64 = 20.0;

/// 1 字の幅の見積もり（論理ピクセル）。制御文字・ゼロ幅は 0
pub fn char_width(c: char) -> f64 {
    if c.is_control() || is_zero_width(c) {
        0.0
    } else if c.is_ascii_uppercase() {
        UPPER_PX
    } else if c == 'm' || c == 'w' {
        WIDE_LOWER_PX
    } else if c.is_ascii_digit() {
        DIGIT_PX
    } else if (c as u32) < 0x1100 || is_halfwidth_form(c) {
        NARROW_PX
    } else {
        WIDE_PX
    }
}

fn is_zero_width(c: char) -> bool {
    matches!(c, '\u{200b}'..='\u{200f}' | '\u{2060}' | '\u{feff}')
}

/// 半角カナ・半角記号（U+FF61〜U+FFDC）
fn is_halfwidth_form(c: char) -> bool {
    matches!(c, '\u{ff61}'..='\u{ffdc}')
}

/// 全角（CJK）か。折り返しはどの字の間でも起きる
fn is_wide(c: char) -> bool {
    char_width(c) == WIDE_PX
}

/// 文字列を 1 行に並べたときの幅の見積もり
pub fn text_width(s: &str) -> f64 {
    s.chars().map(char_width).sum()
}

/// 折り返した結果
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wrapped {
    /// 行数（1 以上）
    pub lines: usize,
    /// いちばん広い行の幅
    pub widest: f64,
    /// 幅が足りずに折った行があるか（改行文字による改行は数えない）
    pub broke: bool,
}

/// CSS `white-space: pre-wrap; overflow-wrap: anywhere` と同じ折り返しの見積もり（Edge で確かめた挙動。
/// .steering/20261027-fix-bubble-size/brief.md）。
/// 改行文字は必ず改行。全角はどの字の間でも折れる。英数字の連なり（語）は丸ごと次の行へ送り、
/// 1 行に入らない語（URL など）は次の行へ送ってから途中で折る（今の行の残りを埋めない）。
/// 「-」の後ろ（次が数字でなければ）は折れるが「/」「.」「_」は折れない。語の後の空白は行末ではぶら下がる（幅に入れない）
pub fn wrap(text: &str, max_width: f64) -> Wrapped {
    // 末尾の改行 1 つは空行を作らない（pre-wrap と同じ）
    let text = text.strip_suffix('\n').unwrap_or(text);
    let mut lines = 0;
    let mut widest: f64 = 0.0;
    let mut broke = false;
    for para in text.split('\n') {
        let w = wrap_paragraph(para, max_width);
        lines += w.lines;
        widest = widest.max(w.widest);
        broke |= w.broke;
    }
    Wrapped {
        lines: lines.max(1),
        widest,
        broke,
    }
}

enum Token {
    Space(f64),
    Word(Vec<char>, f64),
}

fn tokens(para: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut word: Vec<char> = Vec::new();
    let flush = |word: &mut Vec<char>, out: &mut Vec<Token>| {
        if !word.is_empty() {
            let w = word.iter().copied().map(char_width).sum();
            out.push(Token::Word(std::mem::take(word), w));
        }
    };
    let mut it = para.chars().peekable();
    while let Some(c) = it.next() {
        if c == ' ' {
            flush(&mut word, &mut out);
            out.push(Token::Space(char_width(c)));
        } else if is_wide(c) {
            flush(&mut word, &mut out);
            out.push(Token::Word(vec![c], char_width(c)));
        } else {
            word.push(c);
            // ハイフンの後ろは折り返せる（UAX #14 の HY。次が数字なら「-5」のような負号とみて折らない）
            if c == '-' && !it.peek().is_some_and(char::is_ascii_digit) {
                flush(&mut word, &mut out);
            }
        }
    }
    flush(&mut word, &mut out);
    out
}

fn wrap_paragraph(para: &str, max_width: f64) -> Wrapped {
    let mut lines = 1;
    let mut widest: f64 = 0.0;
    let mut broke = false;
    let mut cur = 0.0;
    let mut pending_space = 0.0;
    for t in tokens(para) {
        match t {
            Token::Space(w) => pending_space += w,
            Token::Word(chars, w) => {
                if cur + pending_space + w <= max_width {
                    cur += pending_space + w;
                } else {
                    if cur > 0.0 {
                        widest = widest.max(cur);
                        lines += 1;
                        broke = true;
                        cur = 0.0;
                    }
                    if w <= max_width {
                        cur = w;
                    } else {
                        // 1 行に入らない語は字の途中で折る（overflow-wrap: anywhere）
                        for cw in chars.into_iter().map(char_width) {
                            if cur + cw > max_width && cur > 0.0 {
                                widest = widest.max(cur);
                                lines += 1;
                                broke = true;
                                cur = 0.0;
                            }
                            cur += cw;
                        }
                    }
                }
                pending_space = 0.0;
            }
        }
    }
    Wrapped {
        lines,
        widest: widest.max(cur),
        broke,
    }
}

/// 固定文言の島の大きさ（論理ピクセル・整数）。
/// 幅: 1 行に収まるなら文字の幅＋左右の余白（180〜320）。折り返したら 320（折り返しの計算と同じ幅
/// にしないと、行の切れ目がずれて行数が増える）。高さ: 上下の余白＋行数×行の高さ（上限なし）
pub fn status_island(text: &str) -> (f64, f64) {
    let w = wrap(text, STATUS_MAX_WIDTH - STATUS_PAD_X * 2.0);
    let width = if w.broke {
        STATUS_MAX_WIDTH
    } else {
        (w.widest + STATUS_PAD_X * 2.0)
            .ceil()
            .clamp(STATUS_MIN_WIDTH, STATUS_MAX_WIDTH)
    };
    let height = STATUS_PAD_Y * 2.0 + STATUS_LINE_HEIGHT * w.lines as f64;
    (width, height)
}
