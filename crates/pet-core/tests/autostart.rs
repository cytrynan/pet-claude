//! 自動起動（Windows タスク）の純粋な部分のテスト。Phase 4・PRD N5（`run-hidden.vbs` 経由で窓を出さない）。
//! 書いた順: このテスト → `pet_core::autostart`。

use pet_core::autostart::{
    AutostartView, TASK_NAME, TaskSpec, decode_output, registered_exe, task_xml, utf16le_with_bom,
    view,
};

const VBS: &str = r"C:\Users\k\.claude\scripts\run-hidden.vbs";
const EXE: &str = r"C:\Users\k\dev\pet-claude\pet-claude-main\target\debug\pet-app.exe";

fn spec<'a>(exe: &'a str) -> TaskSpec<'a> {
    TaskSpec {
        user_id: r"PC\k",
        vbs: VBS,
        exe,
        enabled: true,
    }
}

#[test]
fn task_name_is_pet_claude() {
    assert_eq!(TASK_NAME, "pet-claude");
}

#[test]
fn starts_at_logon_of_this_user_only() {
    let xml = task_xml(&spec(EXE)).unwrap();
    assert!(xml.contains("<LogonTrigger>"));
    assert!(xml.contains(r"<UserId>PC\k</UserId>"));
    // ログオン直後はタスクバーがまだ無いことがあるので少し待つ
    assert!(xml.contains("<Delay>PT10S</Delay>"));
    assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
    assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"));
}

#[test]
fn runs_through_run_hidden_vbs_without_a_window() {
    let xml = task_xml(&spec(EXE)).unwrap();
    assert!(xml.contains(r"<Command>%SystemRoot%\System32\wscript.exe</Command>"));
    let args = format!(r#"<Arguments>//B //Nologo "{VBS}" pet-claude "{EXE}"</Arguments>"#);
    assert!(xml.contains(&args), "{xml}");
    assert!(xml.contains(r"<WorkingDirectory>C:\Users\k\dev\pet-claude\pet-claude-main\target\debug</WorkingDirectory>"));
}

#[test]
fn keeps_running_forever_and_on_battery() {
    let xml = task_xml(&spec(EXE)).unwrap();
    // 既定の 72 時間で止められないように（常駐するので時間の上限なし）
    assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
    assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
    assert!(xml.contains("<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>"));
    assert!(xml.contains("<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>"));
    // タスクの既定の優先度 7（通常未満）だとペットの動きが他の負荷に負ける
    assert!(xml.contains("<Priority>4</Priority>"));
    assert!(xml.contains("<Enabled>true</Enabled>"));
}

#[test]
fn disabled_task_for_tests() {
    let xml = task_xml(&TaskSpec {
        enabled: false,
        ..spec(EXE)
    })
    .unwrap();
    assert!(xml.contains("<Enabled>false</Enabled>"));
    assert!(!xml.contains("<Enabled>true</Enabled>"));
}

#[test]
fn escapes_xml_special_characters() {
    let exe = r"C:\R&D\<pet>\pet-app.exe";
    let xml = task_xml(&TaskSpec {
        user_id: "PC\\a&b",
        ..spec(exe)
    })
    .unwrap();
    assert!(xml.contains(r"<UserId>PC\a&amp;b</UserId>"));
    assert!(xml.contains(r"C:\R&amp;D\&lt;pet&gt;\pet-app.exe"));
    assert!(!xml.contains("R&D"));
}

#[test]
fn rejects_paths_that_cannot_be_quoted_or_are_not_absolute() {
    assert!(task_xml(&spec(r#"C:\a"b\pet-app.exe"#)).is_err());
    assert!(task_xml(&spec("")).is_err());
    assert!(task_xml(&spec(r"target\debug\pet-app.exe")).is_err());
    assert!(
        task_xml(&TaskSpec {
            vbs: "run-hidden.vbs",
            ..spec(EXE)
        })
        .is_err()
    );
    assert!(
        task_xml(&TaskSpec {
            user_id: "",
            ..spec(EXE)
        })
        .is_err()
    );
    // 改行や制御文字は引数を割るので受けない
    assert!(task_xml(&spec("C:\\a\nb\\pet-app.exe")).is_err());
    // UNC は絶対パスとして受ける
    assert!(task_xml(&spec(r"\\nas\share\pet-app.exe")).is_ok());
}

#[test]
fn reads_back_the_registered_exe() {
    let xml = task_xml(&spec(EXE)).unwrap();
    assert_eq!(registered_exe(&xml).as_deref(), Some(EXE));
    let odd = r"C:\R&D\pet-app.exe";
    let xml = task_xml(&spec(odd)).unwrap();
    assert_eq!(registered_exe(&xml).as_deref(), Some(odd));
}

#[test]
fn reads_back_a_task_made_by_hand() {
    // run-hidden.vbs を通さず exe を直接登録したタスク
    let xml =
        r#"<Task><Actions><Exec><Command>"C:\x\pet-app.exe"</Command></Exec></Actions></Task>"#;
    assert_eq!(registered_exe(xml).as_deref(), Some(r"C:\x\pet-app.exe"));
    assert_eq!(registered_exe("<Task></Task>"), None);
}

#[test]
fn view_compares_paths_case_insensitively() {
    assert_eq!(view(None, EXE), AutostartView::Off);
    assert_eq!(view(Some(EXE), EXE), AutostartView::On);
    assert_eq!(view(Some(&EXE.to_uppercase()), EXE), AutostartView::On);
    assert_eq!(view(Some(&EXE.replace('\\', "/")), EXE), AutostartView::On);
    let other = r"C:\old\pet-app.exe";
    assert_eq!(
        view(Some(other), EXE),
        AutostartView::OtherExe(other.to_string())
    );
}

#[test]
fn decodes_schtasks_output_in_either_encoding() {
    let s = "<Task>ペット</Task>";
    assert_eq!(decode_output(&utf16le_with_bom(s)), s);
    // BOM なしの UTF-16LE（ASCII の後ろに NUL が並ぶ）
    let no_bom: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
    assert_eq!(decode_output(&no_bom), s);
    assert_eq!(decode_output(s.as_bytes()), s);
    assert_eq!(decode_output(&[]), "");
}

#[test]
fn utf16_file_starts_with_bom() {
    let b = utf16le_with_bom("<a/>");
    assert_eq!(&b[..2], &[0xFF, 0xFE]);
    assert_eq!(b.len(), 2 + 4 * 2);
}
