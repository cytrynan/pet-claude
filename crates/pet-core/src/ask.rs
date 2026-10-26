//! Claude Code の質問（`AskUserQuestion`）にペットの吹き出しで答えるための、OS に触らない部分。
//!
//! - `PreToolUse`（`tool_name == "AskUserQuestion"`）の `tool_input.questions` を解析する（`AskRequest`）
//! - 吹き出しに出してよいかを検査する（承認の要約と同じ 1 行化＋チャットの吹き出しと同じ URL/パス/コード/秘密らしい
//!   文字列の拒否。出せなければ Claude Code のいつもの画面に任せる）
//! - 答え（選んだラベル・自由文・拒否・任せる）から hook の出力 JSON を組む（`ask_output`）。公式の形は
//!   `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","updatedInput":{"questions":[元のまま],"answers":{"<質問文>":"<ラベル>"}}}}`
//!   （code.claude.com/docs/en/hooks「Tools that require user interaction」。複数選択はラベルを `, ` でつなぐ。
//!   選択肢に無い自由文も受けることを 2026-10-27 に CLI の対話で実測した）
//! - 1 問ずつ答えを集める `Progress`（アプリの吹き出しが使う。Tauri に依存しないのでここでテストする）
//!
//! 入力は全部信頼できない外部データなので、どの経路でも panic しない。仕様の正本は
//! `.steering/20261027-ask-question/brief.md`。

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::chat::is_unsafe_for_bubble;
use crate::hook::sanitize_line;

/// Claude Code が 1 回に聞く質問の上限（公式の AskUserQuestion の仕様に合わせる。超えたら出さない）。
pub const MAX_QUESTIONS: usize = 4;
/// 1 問の選択肢の上限（同上）。
pub const MAX_OPTIONS: usize = 4;
/// カードに出す質問文の最大文字数（Unicode のスカラー値。超えたら切り詰めて `…`。全文はターミナルで）。
pub const QUESTION_MAX_CHARS: usize = 200;
/// 見出し（`header`。公式は 12 字以内）の最大文字数。
pub const HEADER_MAX_CHARS: usize = 24;
/// 選択肢のラベルの最大文字数。
pub const LABEL_MAX_CHARS: usize = 60;
/// 選択肢の説明の最大文字数（カードでは 1 行に省略する）。
pub const DESCRIPTION_MAX_CHARS: usize = 80;
/// 「回答」の自由文の最大文字数（チャット欄の 2000 字より短くする。答えは 1 行で返す）。
pub const FREE_TEXT_MAX_CHARS: usize = 500;
/// 「拒否」で Claude に届ける理由（承認の拒否と同じ文言）。
pub const DENY_REASON: &str = "ペット（pet-claude）の吹き出しで拒否されました";
/// 複数選択のラベルのつなぎ（公式「カンマでつなぐ」）。
const SEPARATOR: &str = ", ";

/// 1 つの選択肢。文字列は元のまま持つ（表示は `display_*`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub label: String,
    #[serde(default)]
    pub description: String,
}

/// 1 つの質問。文字列は元のまま持つ（`question` は hook の出力で answers の鍵になる）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub question: String,
    #[serde(default)]
    pub header: String,
    pub options: Vec<Choice>,
    #[serde(default, rename = "multiSelect")]
    pub multi_select: bool,
}

/// 解析した `AskUserQuestion` の入力。`raw` は `tool_input.questions` そのもの（出力で元のまま戻す）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskRequest {
    pub questions: Vec<Question>,
    raw: Value,
}

/// 1 問の答え。アプリから hook へは JSON（`{"chosen":[...]}`／`{"free":"..."}`）で渡す。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reply {
    /// 選んだラベル（1 つ選ぶ質問は 1 つ、複数選択は 1 つ以上）。
    Chosen(Vec<String>),
    /// 「回答」の自由文。
    Free(String),
}

/// hook が返す答え。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskAnswer {
    /// 全部の質問に答えた（質問と同じ順）。
    Answered(Vec<Reply>),
    /// 拒否（Claude に `DENY_REASON` が届く）。
    Deny,
    /// 決めない（Claude Code のいつもの画面に任せる）。
    Ask,
}

/// 見えない・並びを変える書式文字（ゼロ幅・双方向制御・BOM）。質問文や選択肢に混ざっていたら出さない
/// （見た目と違う文で答えさせないため。改行・タブは `sanitize_line` が空白にするので対象外）。
fn has_hidden_chars(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(c,
            '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}' | '\u{00AD}'
        )
    })
}

/// 表示用の 1 行にして、空でなく、URL/パス/コード/秘密らしい文字列と隠れた文字が無ければ true。
fn is_showable(s: &str) -> bool {
    let line = sanitize_line(s, usize::MAX);
    !line.is_empty() && !has_hidden_chars(&line) && !is_unsafe_for_bubble(&line)
}

impl Choice {
    /// カードのボタンに出すラベル（1 行・`LABEL_MAX_CHARS`）。
    pub fn display_label(&self) -> String {
        sanitize_line(&self.label, LABEL_MAX_CHARS)
    }

    /// カードに出す説明（1 行・`DESCRIPTION_MAX_CHARS`）。URL/パス/コード/秘密らしい文字列や隠れた文字を
    /// 含む説明は出さず空にする（説明だけなら隠してもラベルで答えられる）。
    pub fn display_description(&self) -> String {
        if is_showable(&self.description) {
            sanitize_line(&self.description, DESCRIPTION_MAX_CHARS)
        } else {
            String::new()
        }
    }
}

impl Question {
    /// カードに出す質問文（1 行・`QUESTION_MAX_CHARS`）。
    pub fn display_question(&self) -> String {
        sanitize_line(&self.question, QUESTION_MAX_CHARS)
    }

    /// カードに出す見出し（1 行・`HEADER_MAX_CHARS`）。説明と同じく、URL/パス/コード/秘密らしい文字列や隠れた
    /// 文字を含む見出しは出さず空にする（アプリは空なら「質問」と出す。2026-10-27 セキュリティレビューの指摘）。
    pub fn display_header(&self) -> String {
        if is_showable(&self.header) {
            sanitize_line(&self.header, HEADER_MAX_CHARS)
        } else {
            String::new()
        }
    }

    /// `label` が選択肢にあるか（元の文字列で比べる）。
    fn has_option(&self, label: &str) -> bool {
        self.options.iter().any(|c| c.label == label)
    }

    /// この質問をカードに出してよいか。質問文と全ラベルが出せる文字列で、ラベルが互いに違う。
    fn presentable(&self) -> bool {
        if !is_showable(&self.question)
            || self.options.is_empty()
            || self.options.len() > MAX_OPTIONS
        {
            return false;
        }
        let mut seen = HashSet::new();
        self.options
            .iter()
            .all(|c| is_showable(&c.label) && seen.insert(c.label.as_str()))
    }
}

fn parse_choice(v: &Value) -> Option<Choice> {
    let obj = v.as_object()?;
    Some(Choice {
        label: obj.get("label")?.as_str()?.to_string(),
        description: match obj.get("description") {
            None | Some(Value::Null) => String::new(),
            Some(d) => d.as_str()?.to_string(),
        },
    })
}

fn parse_question(v: &Value) -> Option<Question> {
    let obj = v.as_object()?;
    let options = obj.get("options")?.as_array()?;
    if options.is_empty() || options.len() > MAX_OPTIONS {
        return None;
    }
    Some(Question {
        question: obj.get("question")?.as_str()?.to_string(),
        header: match obj.get("header") {
            None | Some(Value::Null) => String::new(),
            Some(h) => h.as_str()?.to_string(),
        },
        options: options
            .iter()
            .map(parse_choice)
            .collect::<Option<Vec<_>>>()?,
        multi_select: match obj.get("multiSelect") {
            None | Some(Value::Null) => false,
            Some(m) => m.as_bool()?,
        },
    })
}

impl AskRequest {
    /// `tool_input` から解析する。形が違う（`questions` が無い・空・5 つ以上、質問文やラベルが文字列でない、
    /// 選択肢が無い・5 つ以上）なら `None`。
    pub fn parse(tool_input: &Value) -> Option<AskRequest> {
        let raw = tool_input.get("questions")?;
        let list = raw.as_array()?;
        if list.is_empty() || list.len() > MAX_QUESTIONS {
            return None;
        }
        let questions = list
            .iter()
            .map(parse_question)
            .collect::<Option<Vec<_>>>()?;
        Some(AskRequest {
            questions,
            raw: raw.clone(),
        })
    }

    /// 元の `questions`（hook の出力で `updatedInput.questions` に戻す）。
    pub fn raw_questions(&self) -> &Value {
        &self.raw
    }

    /// 吹き出しに出してよいか（`presentable`）。
    pub fn presentable(&self) -> bool {
        presentable(&self.questions)
    }
}

/// 吹き出しに出してよいか。1〜4 問で、全部の質問が出せて、質問文が互いに違う（answers の鍵が衝突しない）。
/// アプリは wire で受けた `Vec<Question>` に対して呼ぶ。
pub fn presentable(questions: &[Question]) -> bool {
    let mut seen = HashSet::new();
    !questions.is_empty()
        && questions.len() <= MAX_QUESTIONS
        && questions
            .iter()
            .all(|q| q.presentable() && seen.insert(q.question.as_str()))
}

/// 「回答」の自由文を整える。前後の空白を落とし 1 行にして、空か `FREE_TEXT_MAX_CHARS` 字超なら `None`
/// （切り詰めず断る。途中で切れた答えは意味が変わる）。
pub fn free_text(s: &str) -> Option<String> {
    let line = sanitize_line(s, usize::MAX);
    (!line.is_empty() && line.chars().count() <= FREE_TEXT_MAX_CHARS).then_some(line)
}

/// 答えの並びが質問に合っているか（数が同じ・ラベルが選択肢にある・1 つ選ぶ質問は 1 つ・重複なし・
/// 自由文は `free_text` を通る）。
fn valid_replies(questions: &[Question], replies: &[Reply]) -> bool {
    questions.len() == replies.len()
        && questions.iter().zip(replies).all(|(q, r)| match r {
            Reply::Chosen(labels) => {
                let mut seen = HashSet::new();
                !labels.is_empty()
                    && (q.multi_select || labels.len() == 1)
                    && labels
                        .iter()
                        .all(|l| q.has_option(l) && seen.insert(l.as_str()))
            }
            Reply::Free(text) => free_text(text).is_some(),
        })
}

/// 1 問の答えを answers の値（文字列）にする。選んだラベルは選択肢の順に並べる（見た目と同じ順）。
fn reply_value(q: &Question, r: &Reply) -> String {
    match r {
        Reply::Chosen(labels) => q
            .options
            .iter()
            .filter(|c| labels.contains(&c.label))
            .map(|c| c.label.as_str())
            .collect::<Vec<_>>()
            .join(SEPARATOR),
        Reply::Free(text) => free_text(text).unwrap_or_default(),
    }
}

/// `PreToolUse`（AskUserQuestion）の hook が標準出力に書く JSON（改行を含まない 1 行）。
///
/// `Ask` と、質問に合わない答えは `{}`（何も決めず、Claude Code のいつもの画面に任せる）。
pub fn ask_output(req: &AskRequest, answer: &AskAnswer) -> String {
    let value = match answer {
        AskAnswer::Ask => json!({}),
        AskAnswer::Deny => json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": DENY_REASON
            }
        }),
        AskAnswer::Answered(replies) => {
            if !valid_replies(&req.questions, replies) {
                return "{}".to_string();
            }
            let mut answers = Map::new();
            for (q, r) in req.questions.iter().zip(replies) {
                answers.insert(q.question.clone(), Value::String(reply_value(q, r)));
            }
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "allow",
                    "updatedInput": { "questions": req.raw, "answers": answers }
                }
            })
        }
    };
    serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string())
}

/// `Progress` の操作の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// 同じ質問のまま（複数選択の付け外し）。
    Stay,
    /// 次の質問へ。
    Next,
    /// 全部答えた（質問と同じ順の答え）。
    Done(Vec<Reply>),
    /// 受け付けない（選択肢に無い・空・もう終わっている）。
    Rejected,
}

/// 1 問ずつ答えを集める。カードは `current()` を出し、ボタンで `pick`／`submit`／`answer_text` を呼ぶ。
#[derive(Debug, Clone)]
pub struct Progress {
    questions: Vec<Question>,
    replies: Vec<Reply>,
    /// 複数選択でいま選んでいるラベル（押した順）。
    picked: Vec<String>,
}

impl Progress {
    pub fn new(questions: Vec<Question>) -> Progress {
        Progress {
            questions,
            replies: Vec::new(),
            picked: Vec::new(),
        }
    }

    /// いま答えている質問の番号（0 から）。全部答えたら `total()` と同じ。
    pub fn cursor(&self) -> usize {
        self.replies.len()
    }

    pub fn total(&self) -> usize {
        self.questions.len()
    }

    /// いま答えている質問。全部答えたら `None`。
    pub fn current(&self) -> Option<&Question> {
        self.questions.get(self.cursor())
    }

    /// 複数選択でいま選んでいるラベル。
    pub fn picked(&self) -> &[String] {
        &self.picked
    }

    /// 答えを 1 つ入れて次へ進む。
    fn advance(&mut self, reply: Reply) -> Step {
        self.replies.push(reply);
        self.picked.clear();
        if self.replies.len() == self.questions.len() {
            Step::Done(self.replies.clone())
        } else {
            Step::Next
        }
    }

    /// 選択肢のボタン。1 つ選ぶ質問ならその答えで次へ、複数選択なら付け外し。
    pub fn pick(&mut self, label: &str) -> Step {
        let Some(q) = self.current() else {
            return Step::Rejected;
        };
        if !q.has_option(label) {
            return Step::Rejected;
        }
        if !q.multi_select {
            return self.advance(Reply::Chosen(vec![label.to_string()]));
        }
        if let Some(i) = self.picked.iter().position(|l| l == label) {
            self.picked.remove(i);
        } else {
            self.picked.push(label.to_string());
        }
        Step::Stay
    }

    /// 複数選択の「決定」。1 つも選んでいない・1 つ選ぶ質問なら受け付けない。
    pub fn submit(&mut self) -> Step {
        let Some(q) = self.current() else {
            return Step::Rejected;
        };
        if !q.multi_select || self.picked.is_empty() {
            return Step::Rejected;
        }
        let picked = std::mem::take(&mut self.picked);
        self.advance(Reply::Chosen(picked))
    }

    /// 「回答」の自由文。選んでいた選択肢は捨てる。
    pub fn answer_text(&mut self, text: &str) -> Step {
        if self.current().is_none() {
            return Step::Rejected;
        }
        match free_text(text) {
            Some(t) => self.advance(Reply::Free(t)),
            None => Step::Rejected,
        }
    }
}
