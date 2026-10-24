//! 自動起動の登録・解除・状態（Phase 4・PRD N5）。中身の XML は `pet_core::autostart` が組み、
//! ここは `schtasks.exe` を窓なしで起こすだけ。設定画面のスイッチと `--install-autostart` から呼ぶ。
//!
//! 本物の状態はタスク スケジューラにあるので `state.json` には持たない（タスクを手で消しても画面がずれない）。

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Output};

use pet_core::autostart::{
    AutostartView, TASK_NAME, TaskSpec, decode_output, registered_exe, task_xml, utf16le_with_bom,
    view,
};
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

use crate::log::{data_dir, log};

/// K の規範の置き場所（`~/.claude/scripts/run-hidden.vbs`）。窓を出さずに起こし、出力をログに残す
fn run_hidden_vbs() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").map_or_else(PathBuf::new, PathBuf::from);
    home.join(r".claude\scripts\run-hidden.vbs")
}

fn schtasks(args: &[&str]) -> Result<Output, String> {
    Command::new("schtasks.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW.0)
        .output()
        .map_err(|e| format!("schtasks を起こせない: {e}"))
}

/// 失敗の 1 行（schtasks は日本語の Windows では CP932 で書くので、読めない字は落ちる）
fn reason(out: &Output) -> String {
    let text = decode_output(&out.stderr);
    let line = text.lines().map(str::trim).find(|l| !l.is_empty());
    line.unwrap_or("理由不明").chars().take(200).collect()
}

fn current_exe() -> Result<String, String> {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| format!("自分の exe の場所が分からない: {e}"))
}

/// 登録されている exe（無ければ None）
fn registered(task: &str) -> Result<Option<String>, String> {
    let out = schtasks(&["/Query", "/TN", task, "/XML"])?;
    if !out.status.success() {
        // 見つからない時も失敗で返る。区別は出力の文言に頼らず、/Query だけで再確認する
        let exists = schtasks(&["/Query", "/TN", task])?.status.success();
        return if exists { Err(reason(&out)) } else { Ok(None) };
    }
    let xml = decode_output(&out.stdout);
    // 登録はあるのに exe を読めない（手で作った別物）。空の文字列として「別の場所」に出す
    Ok(Some(registered_exe(&xml).unwrap_or_default()))
}

/// 設定画面に出す状態
pub fn status(task: &str) -> Result<AutostartView, String> {
    Ok(view(registered(task)?.as_deref(), &current_exe()?))
}

/// いま動いている pet-app を登録する（既にあれば置き換える）
pub fn enable(task: &str, enabled: bool) -> Result<(), String> {
    let vbs = run_hidden_vbs();
    if !vbs.is_file() {
        return Err(format!("{} が無い", vbs.display()));
    }
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(d), Ok(u)) => format!("{d}\\{u}"),
        _ => return Err("ユーザー名が分からない".into()),
    };
    let exe = current_exe()?;
    let xml = task_xml(&TaskSpec {
        user_id: &user,
        vbs: &vbs.to_string_lossy(),
        exe: &exe,
        enabled,
    })?;
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("autostart-task.xml");
    std::fs::write(&file, utf16le_with_bom(&xml)).map_err(|e| format!("XML を書けない: {e}"))?;
    let path = file.to_string_lossy().into_owned();
    let out = schtasks(&["/Create", "/TN", task, "/XML", &path, "/F"]);
    let _ = std::fs::remove_file(&file);
    let out = out?;
    if !out.status.success() {
        return Err(reason(&out));
    }
    log(format!("autostart: 登録 {task} enabled={enabled}"));
    Ok(())
}

/// 登録を外す（無ければ何もしない）
pub fn disable(task: &str) -> Result<(), String> {
    if registered(task)?.is_none() {
        return Ok(());
    }
    let out = schtasks(&["/Delete", "/TN", task, "/F"])?;
    if !out.status.success() {
        return Err(reason(&out));
    }
    log(format!("autostart: 解除 {task}"));
    Ok(())
}

/// 設定画面に返す形: `{ state: "on" | "off" | "other" | "error", detail }`
pub fn status_json() -> serde_json::Value {
    let (state, detail) = match status(TASK_NAME) {
        Ok(AutostartView::On) => ("on", String::new()),
        Ok(AutostartView::Off) => ("off", String::new()),
        Ok(AutostartView::OtherExe(p)) => ("other", p),
        Err(e) => ("error", e),
    };
    serde_json::json!({ "state": state, "detail": detail })
}

/// 設定画面のスイッチ。失敗したら理由をログに残し、状態は読み直して返す（失敗しても画面と実物をずらさない）
pub fn set_json(on: bool) -> serde_json::Value {
    let result = if on {
        enable(TASK_NAME, true)
    } else {
        disable(TASK_NAME)
    };
    let mut v = status_json();
    if let Err(e) = result {
        log(format!("autostart: 失敗 {e}"));
        v["state"] = "error".into();
        v["detail"] = e.into();
    }
    v
}

/// `--install-autostart [--task-name <名前>] [--disabled]` / `--uninstall-autostart` / `--autostart-status`
pub fn run_cli(args: &[String]) -> String {
    let value = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let task = value("--task-name").unwrap_or_else(|| TASK_NAME.to_string());
    let has = |name: &str| args.iter().any(|a| a == name);
    let result = if has("--install-autostart") {
        enable(&task, !has("--disabled")).map(|()| format!("登録した: {task}"))
    } else if has("--uninstall-autostart") {
        disable(&task).map(|()| format!("外した: {task}"))
    } else {
        status(&task).map(|v| format!("{task}: {v:?}"))
    };
    result.unwrap_or_else(|e| format!("失敗: {e}"))
}
