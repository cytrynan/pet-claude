//! `pet_core::voice::speech_chunks`（読み上げを文に分ける）の単体テスト。
//! 2026-10-22 K の「話し出すまでの時間を短縮できないの？」から足した（最初の文ができたら話し始める）。

use pet_core::voice::speech_chunks;

#[test]
fn splits_after_sentence_marks() {
    assert_eq!(
        speech_chunks("明日は雨が降りそうだよ。傘を持っていってね！上着もあるといいかな？"),
        vec![
            "明日は雨が降りそうだよ。",
            "傘を持っていってね！",
            "上着もあるといいかな？"
        ]
    );
}

#[test]
fn short_sentences_join_the_next() {
    assert_eq!(
        speech_chunks("うん。明日は雨が降りそうだよ。"),
        vec!["うん。明日は雨が降りそうだよ。"]
    );
}

#[test]
fn short_tail_joins_the_previous() {
    assert_eq!(
        speech_chunks("明日は雨が降りそうだよ。うん。"),
        vec!["明日は雨が降りそうだよ。うん。"]
    );
}

#[test]
fn no_marks_is_one_chunk() {
    assert_eq!(speech_chunks("できたよ"), vec!["できたよ"]);
}

#[test]
fn only_short_text_is_one_chunk() {
    assert_eq!(speech_chunks("うん。"), vec!["うん。"]);
}

#[test]
fn empty_and_blank_give_nothing() {
    assert!(speech_chunks("").is_empty());
    assert!(speech_chunks("   ").is_empty());
}

#[test]
fn ascii_marks_and_spaces_are_trimmed() {
    assert_eq!(
        speech_chunks(" Is it raining today? Yes it is! "),
        vec!["Is it raining today?", "Yes it is!"]
    );
}

#[test]
fn joined_chunks_keep_every_character() {
    let inputs = [
        "明日は雨。傘を持ってね。上着も。うん。そうだね！ほんとに？",
        "。。。！？",
        "あ。い。う。え。お。か。き。く。け。こ。",
        "長い文の途中に、読点があっても切らないよ。次の文。",
    ];
    for t in inputs {
        let joined: String = speech_chunks(t).concat();
        let want: String = t.chars().filter(|c| !c.is_whitespace()).collect();
        let got: String = joined.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(got, want, "{t}");
    }
}
