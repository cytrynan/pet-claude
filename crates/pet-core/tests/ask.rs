//! `pet_core::ask` の受け入れテスト（テスト先行）。
//!
//! 原本: `.steering/20261027-ask-question/brief.md`。Claude Code の `AskUserQuestion` の `PreToolUse` hook の
//! 入力（`tool_input.questions`）を解析し、吹き出しに出してよいかを検査し、答えから hook の出力 JSON を組む。
//! 公開 API だけを使う。

use pet_core::ask::{
    AskAnswer, AskRequest, Choice, DENY_REASON, DESCRIPTION_MAX_CHARS, FREE_TEXT_MAX_CHARS,
    HEADER_MAX_CHARS, LABEL_MAX_CHARS, MAX_OPTIONS, MAX_QUESTIONS, Progress, QUESTION_MAX_CHARS,
    Question, Reply, Step, ask_output, free_text,
};
use serde_json::{Value, json};

/// 司令塔が 2026-10-27 に実測した hook 入力の `tool_input`（askq-hook-input.jsonl）。
fn color_input() -> Value {
    json!({ "questions": [ {
        "question": "どの色が好きですか？", "header": "好きな色",
        "options": [
            { "label": "Red", "description": "赤" },
            { "label": "Green", "description": "緑" },
            { "label": "Blue", "description": "青" }
        ],
        "multiSelect": false
    } ] })
}

fn two_questions() -> Value {
    json!({ "questions": [
        { "question": "どの色？", "header": "色", "options": [ { "label": "Red" }, { "label": "Blue" } ], "multiSelect": false },
        { "question": "どの形？", "header": "形", "options": [ { "label": "丸", "description": "まる" }, { "label": "四角" }, { "label": "三角" } ], "multiSelect": true }
    ] })
}

fn q(question: &str, labels: &[&str], multi: bool) -> Question {
    Question {
        question: question.to_string(),
        header: String::new(),
        options: labels
            .iter()
            .map(|l| Choice {
                label: l.to_string(),
                description: String::new(),
            })
            .collect(),
        multi_select: multi,
    }
}

fn output_value(req: &AskRequest, answer: &AskAnswer) -> Value {
    serde_json::from_str(&ask_output(req, answer)).expect("hook の出力は JSON")
}

// ---------------------------------------------------------------------------
// 識別子契約: 定数
// ---------------------------------------------------------------------------

#[test]
fn constants_have_contract_values() {
    assert_eq!(MAX_QUESTIONS, 4);
    assert_eq!(MAX_OPTIONS, 4);
    assert_eq!(QUESTION_MAX_CHARS, 200);
    assert_eq!(HEADER_MAX_CHARS, 24);
    assert_eq!(LABEL_MAX_CHARS, 60);
    assert_eq!(DESCRIPTION_MAX_CHARS, 80);
    assert_eq!(FREE_TEXT_MAX_CHARS, 500);
    assert_eq!(
        DENY_REASON,
        "ペット（pet-claude）の吹き出しで拒否されました"
    );
}

// ---------------------------------------------------------------------------
// 1. 解析
// ---------------------------------------------------------------------------

#[test]
fn parse_real_input() {
    let req = AskRequest::parse(&color_input()).expect("実測の入力は解析できる");
    assert_eq!(req.questions.len(), 1);
    let q = &req.questions[0];
    assert_eq!(q.question, "どの色が好きですか？");
    assert_eq!(q.header, "好きな色");
    assert!(!q.multi_select);
    assert_eq!(
        q.options
            .iter()
            .map(|c| (c.label.as_str(), c.description.as_str()))
            .collect::<Vec<_>>(),
        vec![("Red", "赤"), ("Green", "緑"), ("Blue", "青")]
    );
    // 元の questions はそのまま持つ（hook の出力の updatedInput に戻すため）
    assert_eq!(req.raw_questions(), &color_input()["questions"]);
    assert!(req.presentable());
}

#[test]
fn parse_defaults_and_shapes() {
    // header・description・multiSelect が無くてもよい
    let req = AskRequest::parse(
        &json!({ "questions": [ { "question": "Q", "options": [ { "label": "A" } ] } ] }),
    )
    .unwrap();
    assert_eq!(req.questions[0].header, "");
    assert_eq!(req.questions[0].options[0].description, "");
    assert!(!req.questions[0].multi_select);
    assert!(req.questions[0].options.len() == 1);
}

#[test]
fn parse_rejects_malformed() {
    let none = |v: Value| assert!(AskRequest::parse(&v).is_none(), "{v}");
    none(json!({}));
    none(json!({ "questions": "x" }));
    none(json!({ "questions": [] }));
    none(json!({ "questions": [ { "options": [ { "label": "A" } ] } ] }));
    none(json!({ "questions": [ { "question": 1, "options": [ { "label": "A" } ] } ] }));
    none(json!({ "questions": [ { "question": "Q" } ] }));
    none(json!({ "questions": [ { "question": "Q", "options": [] } ] }));
    none(
        json!({ "questions": [ { "question": "Q", "options": [ { "description": "no label" } ] } ] }),
    );
    none(json!({ "questions": [ { "question": "Q", "options": [ { "label": 2 } ] } ] }));
    none(json!({ "questions": [ { "question": "Q", "options": "x" } ] }));
    none(Value::Null);
    // 5 問・5 選択肢は Claude Code の上限（4）を超える
    let five_q: Vec<Value> = (0..5)
        .map(|i| json!({ "question": format!("Q{i}"), "options": [ { "label": "A" } ] }))
        .collect();
    none(json!({ "questions": five_q }));
    let five_o: Vec<Value> = (0..5)
        .map(|i| json!({ "label": format!("L{i}") }))
        .collect();
    none(json!({ "questions": [ { "question": "Q", "options": five_o } ] }));
    // multiSelect が bool でない
    none(
        json!({ "questions": [ { "question": "Q", "options": [ { "label": "A" } ], "multiSelect": "yes" } ] }),
    );
}

// ---------------------------------------------------------------------------
// 2. 吹き出しに出してよいか（承認の要約と同じ 1 行化＋チャットの吹き出しと同じ拒否）
// ---------------------------------------------------------------------------

fn with_question(text: &str) -> AskRequest {
    AskRequest::parse(&json!({ "questions": [ { "question": text, "options": [ { "label": "A" }, { "label": "B" } ] } ] }))
        .unwrap()
}

#[test]
fn unsafe_question_is_not_presentable() {
    for bad in [
        "開く？ https://example.com",
        "www.example.com を見る？",
        "`rm -rf` を実行する？",
        "C:\\Users\\k\\secret.txt を消す？",
        "~/notes を読む？",
        "鍵は sk-ant-abc で合ってる？",
        "   ",
        "",
    ] {
        assert!(!with_question(bad).presentable(), "{bad:?}");
    }
    assert!(with_question("どれにする？").presentable());
}

#[test]
fn hidden_chars_are_not_presentable() {
    for c in ['\u{200B}', '\u{200F}', '\u{202E}', '\u{2066}', '\u{FEFF}'] {
        assert!(
            !with_question(&format!("どれ{c}にする？")).presentable(),
            "U+{:04X}",
            c as u32
        );
    }
    // 改行・タブは 1 行に整えるだけで、出してよい
    assert!(with_question("どれに\nする？\t").presentable());
}

#[test]
fn unsafe_label_is_not_presentable_but_unsafe_description_is_just_hidden() {
    let req = AskRequest::parse(&json!({ "questions": [ { "question": "Q", "options": [ { "label": "https://a" }, { "label": "B" } ] } ] })).unwrap();
    assert!(!req.presentable());
    let req = AskRequest::parse(&json!({ "questions": [ { "question": "Q", "options": [ { "label": "A", "description": "see https://a" }, { "label": "B", "description": "ok" } ] } ] })).unwrap();
    assert!(req.presentable());
    assert_eq!(req.questions[0].options[0].display_description(), "");
    assert_eq!(req.questions[0].options[1].display_description(), "ok");
}

#[test]
fn duplicate_labels_or_questions_are_not_presentable() {
    // 同じラベルが 2 つあると、どれを選んだか決められない
    let req = AskRequest::parse(&json!({ "questions": [ { "question": "Q", "options": [ { "label": "A" }, { "label": "A" } ] } ] })).unwrap();
    assert!(!req.presentable());
    // 空のラベル（空白だけ）も同じ
    let req = AskRequest::parse(&json!({ "questions": [ { "question": "Q", "options": [ { "label": " " }, { "label": "B" } ] } ] })).unwrap();
    assert!(!req.presentable());
    // 同じ質問文が 2 つあると answers の鍵が衝突する
    let req = AskRequest::parse(&json!({ "questions": [
        { "question": "Q", "options": [ { "label": "A" } ] },
        { "question": "Q", "options": [ { "label": "B" } ] }
    ] }))
    .unwrap();
    assert!(!req.presentable());
}

#[test]
fn display_texts_are_single_line_and_bounded() {
    let long_q = "あ".repeat(QUESTION_MAX_CHARS + 1);
    let req = AskRequest::parse(&json!({ "questions": [ {
        "question": format!("  {long_q}\n"), "header": "x".repeat(HEADER_MAX_CHARS + 5),
        // 英数字の 32 字以上の並びは「秘密らしい」として隠れるので、長さの検査は日本語で
        "options": [ { "label": format!("ら{}", "ら".repeat(LABEL_MAX_CHARS)), "description": format!("a\tb {}", "ざ".repeat(DESCRIPTION_MAX_CHARS)) } ]
    } ] })).unwrap();
    let q = &req.questions[0];
    let dq = q.display_question();
    assert_eq!(dq.chars().count(), QUESTION_MAX_CHARS);
    assert!(dq.ends_with('…'));
    assert_eq!(q.display_header().chars().count(), HEADER_MAX_CHARS);
    assert_eq!(
        q.options[0].display_label().chars().count(),
        LABEL_MAX_CHARS
    );
    let dd = q.options[0].display_description();
    assert_eq!(dd.chars().count(), DESCRIPTION_MAX_CHARS);
    assert!(dd.starts_with("a b "));
    // 元の文字列は変えない（answers の鍵に使う）
    assert!(q.question.starts_with("  あ"));
    // 長い質問は出してよい（切り詰めて出す。全文はターミナルで）
    assert!(req.presentable());
}

// ---------------------------------------------------------------------------
// 3. 自由文
// ---------------------------------------------------------------------------

#[test]
fn free_text_is_trimmed_single_line_and_bounded() {
    assert_eq!(free_text("  むらさき \n"), Some("むらさき".to_string()));
    assert_eq!(free_text("a\tb  c"), Some("a b c".to_string()));
    assert_eq!(free_text(""), None);
    assert_eq!(free_text("   \n"), None);
    let max = "x".repeat(FREE_TEXT_MAX_CHARS);
    assert_eq!(free_text(&max).as_deref(), Some(max.as_str()));
    assert_eq!(free_text(&"x".repeat(FREE_TEXT_MAX_CHARS + 1)), None);
}

// ---------------------------------------------------------------------------
// 4. hook の出力
// ---------------------------------------------------------------------------

#[test]
fn ask_and_deny_outputs() {
    let req = AskRequest::parse(&color_input()).unwrap();
    assert_eq!(ask_output(&req, &AskAnswer::Ask), "{}");
    let v = output_value(&req, &AskAnswer::Deny);
    assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PreToolUse");
    assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
    assert_eq!(
        v["hookSpecificOutput"]["permissionDecisionReason"],
        DENY_REASON
    );
    assert!(v["hookSpecificOutput"].get("updatedInput").is_none());
    // 1 行（改行を含まない）
    assert!(!ask_output(&req, &AskAnswer::Deny).contains('\n'));
}

#[test]
fn chosen_answer_output_matches_official_shape() {
    let req = AskRequest::parse(&color_input()).unwrap();
    let v = output_value(
        &req,
        &AskAnswer::Answered(vec![Reply::Chosen(vec!["Blue".into()])]),
    );
    let out = &v["hookSpecificOutput"];
    assert_eq!(out["hookEventName"], "PreToolUse");
    assert_eq!(out["permissionDecision"], "allow");
    assert_eq!(out["updatedInput"]["questions"], color_input()["questions"]);
    assert_eq!(
        out["updatedInput"]["answers"],
        json!({ "どの色が好きですか？": "Blue" })
    );
    assert_eq!(out["updatedInput"].as_object().map(|o| o.len()), Some(2));
}

#[test]
fn multi_select_joins_labels_and_free_text_passes_through() {
    let req = AskRequest::parse(&two_questions()).unwrap();
    let v = output_value(
        &req,
        &AskAnswer::Answered(vec![
            Reply::Free("むらさき".into()),
            Reply::Chosen(vec!["丸".into(), "三角".into()]),
        ]),
    );
    assert_eq!(
        v["hookSpecificOutput"]["updatedInput"]["answers"],
        json!({ "どの色？": "むらさき", "どの形？": "丸, 三角" })
    );
    // 選んだ順ではなく選択肢の順で並べる（見た目と同じ順）
    let v = output_value(
        &req,
        &AskAnswer::Answered(vec![
            Reply::Free("x".into()),
            Reply::Chosen(vec!["三角".into(), "丸".into()]),
        ]),
    );
    assert_eq!(
        v["hookSpecificOutput"]["updatedInput"]["answers"]["どの形？"],
        "丸, 三角"
    );
}

#[test]
fn invalid_answers_fall_back_to_the_usual_screen() {
    let req = AskRequest::parse(&two_questions()).unwrap();
    let bad = |replies: Vec<Reply>| {
        assert_eq!(ask_output(&req, &AskAnswer::Answered(replies)), "{}");
    };
    // 数が合わない
    bad(vec![]);
    bad(vec![Reply::Chosen(vec!["Red".into()])]);
    bad(vec![
        Reply::Chosen(vec!["Red".into()]),
        Reply::Chosen(vec!["丸".into()]),
        Reply::Chosen(vec!["丸".into()]),
    ]);
    // 選択肢に無いラベル
    bad(vec![
        Reply::Chosen(vec!["Purple".into()]),
        Reply::Chosen(vec!["丸".into()]),
    ]);
    // 1 つ選ぶ質問に 2 つ・空
    bad(vec![
        Reply::Chosen(vec!["Red".into(), "Blue".into()]),
        Reply::Chosen(vec!["丸".into()]),
    ]);
    bad(vec![
        Reply::Chosen(vec![]),
        Reply::Chosen(vec!["丸".into()]),
    ]);
    // 複数選択に空・重複
    bad(vec![
        Reply::Chosen(vec!["Red".into()]),
        Reply::Chosen(vec![]),
    ]);
    bad(vec![
        Reply::Chosen(vec!["Red".into()]),
        Reply::Chosen(vec!["丸".into(), "丸".into()]),
    ]);
    // 自由文が空・長すぎる
    bad(vec![
        Reply::Free("  ".into()),
        Reply::Chosen(vec!["丸".into()]),
    ]);
    bad(vec![
        Reply::Free("x".repeat(FREE_TEXT_MAX_CHARS + 1)),
        Reply::Chosen(vec!["丸".into()]),
    ]);
    // 正しければ通る
    assert_ne!(
        ask_output(
            &req,
            &AskAnswer::Answered(vec![
                Reply::Chosen(vec!["Red".into()]),
                Reply::Chosen(vec!["丸".into()])
            ])
        ),
        "{}"
    );
}

#[test]
fn free_text_in_output_is_normalized() {
    let req = AskRequest::parse(&color_input()).unwrap();
    let v = output_value(
        &req,
        &AskAnswer::Answered(vec![Reply::Free("  むらさき\n かな ".into())]),
    );
    assert_eq!(
        v["hookSpecificOutput"]["updatedInput"]["answers"]["どの色が好きですか？"],
        "むらさき かな"
    );
}

// ---------------------------------------------------------------------------
// 5. Reply の wire 表現（アプリ → hook）
// ---------------------------------------------------------------------------

#[test]
fn reply_wire_shape() {
    assert_eq!(
        serde_json::to_value(Reply::Chosen(vec!["Blue".into()])).unwrap(),
        json!({ "chosen": ["Blue"] })
    );
    assert_eq!(
        serde_json::to_value(Reply::Free("x".into())).unwrap(),
        json!({ "free": "x" })
    );
    let back: Vec<Reply> =
        serde_json::from_value(json!([{ "chosen": ["A", "B"] }, { "free": "y" }])).unwrap();
    assert_eq!(
        back,
        vec![
            Reply::Chosen(vec!["A".into(), "B".into()]),
            Reply::Free("y".into())
        ]
    );
    assert!(serde_json::from_value::<Reply>(json!({ "other": 1 })).is_err());
}

// ---------------------------------------------------------------------------
// 6. 1 問ずつ答えを集める
// ---------------------------------------------------------------------------

#[test]
fn progress_single_choice_questions() {
    let mut p = Progress::new(vec![
        q("Q1", &["A", "B"], false),
        q("Q2", &["C", "D"], false),
    ]);
    assert_eq!((p.cursor(), p.total()), (0, 2));
    assert_eq!(p.current().map(|q| q.question.as_str()), Some("Q1"));
    assert_eq!(p.pick("Z"), Step::Rejected);
    assert_eq!(p.cursor(), 0);
    assert_eq!(p.pick("B"), Step::Next);
    assert_eq!(p.cursor(), 1);
    assert_eq!(p.current().map(|q| q.question.as_str()), Some("Q2"));
    assert_eq!(
        p.pick("C"),
        Step::Done(vec![
            Reply::Chosen(vec!["B".into()]),
            Reply::Chosen(vec!["C".into()])
        ])
    );
    // 終わった後は何も受け付けない
    assert_eq!(p.pick("C"), Step::Rejected);
    assert_eq!(p.submit(), Step::Rejected);
    assert_eq!(p.answer_text("x"), Step::Rejected);
    assert!(p.current().is_none());
}

#[test]
fn progress_multi_select_toggles_then_submits() {
    let mut p = Progress::new(vec![q("Q", &["A", "B", "C"], true)]);
    // 空のまま決定はできない
    assert_eq!(p.submit(), Step::Rejected);
    assert_eq!(p.pick("A"), Step::Stay);
    assert_eq!(p.pick("C"), Step::Stay);
    assert_eq!(p.picked(), ["A", "C"]);
    // もう一度押すと外れる
    assert_eq!(p.pick("A"), Step::Stay);
    assert_eq!(p.picked(), ["C"]);
    assert_eq!(p.pick("Z"), Step::Rejected);
    assert_eq!(p.pick("B"), Step::Stay);
    assert_eq!(
        p.submit(),
        Step::Done(vec![Reply::Chosen(vec!["C".into(), "B".into()])])
    );
}

#[test]
fn progress_free_text_answers_the_current_question() {
    let mut p = Progress::new(vec![q("Q1", &["A"], true), q("Q2", &["B"], false)]);
    assert_eq!(p.pick("A"), Step::Stay);
    // 自由文は選んでいた選択肢を捨てて、その問いの答えになる
    assert_eq!(p.answer_text("  "), Step::Rejected);
    assert_eq!(p.answer_text(" 自由に "), Step::Next);
    assert!(p.picked().is_empty());
    assert_eq!(
        p.answer_text("二つ目"),
        Step::Done(vec![
            Reply::Free("自由に".into()),
            Reply::Free("二つ目".into())
        ])
    );
}

#[test]
fn progress_single_choice_submit_is_rejected() {
    let mut p = Progress::new(vec![q("Q", &["A"], false)]);
    assert_eq!(p.submit(), Step::Rejected);
    assert!(p.picked().is_empty());
}

#[test]
fn progress_with_no_questions_is_done_from_the_start() {
    let mut p = Progress::new(vec![]);
    assert!(p.current().is_none());
    assert_eq!(p.pick("A"), Step::Rejected);
}
