//! `--install-hooks [--settings <path>]` / `--uninstall-hooks [--settings <path>]`（Phase 2）。
//!
//! Claude Code の settings.json に pet-hook を印（`--pet-claude-managed`）付きで足す・外す。
//! - 足す前に印付きの項目を全部消す（何度実行しても 1 組だけ）。印の無い項目には触らない
//! - 書く前に元のファイルを `<名前>.pet-claude-backup-<時刻>` に複製し、一時ファイル＋rename で置き換える
//! - JSON として読めないファイルは書き換えない（壊さない）
//!
//! 既定の書き先は `~/.claude/settings.json`。試験では `--settings` で作業用のファイルを指す。

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

pub const MARKER: &str = "--pet-claude-managed";

/// 質問（AskUserQuestion）を扱う登録に付ける引数。pet-hook はこれが無い AskUserQuestion の PreToolUse では
/// 何もしない（`*` の非同期登録と二重に送らない）。
pub const ASK_ARG: &str = "--ask";

/// (hook の名前, 待つか, timeout 秒, matcher, 追加の引数)。docs/architecture.md §5.1 と同じ。
/// 質問の timeout は 300 秒（既定の 60 秒では人が読んで考える時間が足りない。K 2026-10-27）
type Registration = (
    &'static str,
    bool,
    u32,
    Option<&'static str>,
    &'static [&'static str],
);
const EVENTS: [Registration; 7] = [
    ("UserPromptSubmit", false, 5, None, &[]),
    ("PreToolUse", false, 5, Some("*"), &[]),
    ("PreToolUse", true, 300, Some("AskUserQuestion"), &[ASK_ARG]),
    ("PermissionRequest", true, 60, Some("*"), &[]),
    ("Notification", false, 5, None, &[]),
    ("Stop", false, 5, None, &[]),
    ("StopFailure", false, 5, None, &[]),
];

fn is_managed(hook: &Value) -> bool {
    let in_args = hook["args"]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v == MARKER));
    let in_command = hook["command"].as_str().is_some_and(|c| c.contains(MARKER));
    in_args || in_command
}

/// 印付きの項目を消す。空になった組・hook 名・`hooks` も消す。
pub fn remove_managed(settings: &mut Value) {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    // 印付きを消して空になったものだけを片付ける。もともと空だった組・hook 名・`hooks` は残す
    // （印の無い設定には触らない。2026-10-21 検証係の指摘）
    let mut emptied_events = Vec::new();
    for (event, groups) in hooks.iter_mut() {
        let Some(groups) = groups.as_array_mut() else {
            continue;
        };
        let groups_before = groups.len();
        groups.retain_mut(|group| {
            let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let before = list.len();
            list.retain(|h| !is_managed(h));
            !(before > 0 && list.is_empty())
        });
        if groups_before > 0 && groups.is_empty() {
            emptied_events.push(event.clone());
        }
    }
    for event in &emptied_events {
        hooks.remove(event);
    }
    if !emptied_events.is_empty()
        && hooks.is_empty()
        && let Some(obj) = settings.as_object_mut()
    {
        obj.remove("hooks");
    }
}

/// 印付きの項目を（消してから）足す。
pub fn add_managed(settings: &mut Value, hook_exe: &str) {
    remove_managed(settings);
    if !settings.is_object() {
        *settings = Value::Object(Map::new());
    }
    let Some(root) = settings.as_object_mut() else {
        return;
    };
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    if !hooks.is_object() {
        *hooks = Value::Object(Map::new());
    }
    let Some(hooks) = hooks.as_object_mut() else {
        return;
    };
    for (event, wait, timeout, matcher, extra_args) in EVENTS {
        let mut args = vec![MARKER];
        args.extend_from_slice(extra_args);
        let mut entry = json!({
            "type": "command",
            "command": hook_exe,
            "args": args,
            "timeout": timeout,
        });
        if !wait {
            entry["async"] = json!(true);
        }
        let mut group = json!({ "hooks": [entry] });
        if let Some(m) = matcher {
            group["matcher"] = json!(m);
        }
        let groups = hooks
            .entry(event)
            .or_insert_with(|| Value::Array(Vec::new()));
        if !groups.is_array() {
            *groups = Value::Array(Vec::new());
        }
        if let Some(groups) = groups.as_array_mut() {
            groups.push(group);
        }
    }
}

fn default_settings_path() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".claude").join("settings.json"))
}

/// pet-app.exe と同じフォルダの pet-hook.exe。
fn hook_exe() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let hook = exe.with_file_name("pet-hook.exe");
    if hook.is_file() {
        Ok(hook)
    } else {
        Err(format!("{} が無い（cargo build で作る）", hook.display()))
    }
}

fn read(path: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(json!({})),
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| format!("{} は JSON として読めないので触らない: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn write(path: &Path, value: &Value) -> Result<Option<PathBuf>, String> {
    let backup = if path.is_file() {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let name = format!(
            "{}.pet-claude-backup-{stamp}",
            path.file_name()
                .map_or_else(Default::default, |n| n.to_string_lossy())
        );
        let b = path.with_file_name(name);
        std::fs::copy(path, &b).map_err(|e| format!("バックアップに失敗: {e}"))?;
        Some(b)
    } else {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        None
    };
    let mut text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    text.push('\n');
    let tmp = path.with_extension("json.pet-claude-tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(backup)
}

/// `--install-hooks` / `--uninstall-hooks` の本体。結果の 1 行を返す。
pub fn run(install: bool, settings: Option<&str>) -> Result<String, String> {
    let path = settings
        .map(PathBuf::from)
        .or_else(default_settings_path)
        .ok_or("settings.json の場所が分からない")?;
    let mut value = read(&path)?;
    if install {
        let hook = hook_exe()?;
        add_managed(&mut value, &hook.to_string_lossy());
    } else {
        remove_managed(&mut value);
    }
    let backup = write(&path, &value)?;
    let verb = if install { "登録した" } else { "外した" };
    Ok(match backup {
        Some(b) => format!(
            "{}: {verb}（元のファイル: {}）",
            path.display(),
            b.display()
        ),
        None => format!("{}: {verb}", path.display()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn managed_count(v: &Value) -> usize {
        v["hooks"].as_object().map_or(0, |h| {
            h.values()
                .flat_map(|g| g.as_array().cloned().unwrap_or_default())
                .flat_map(|g| g["hooks"].as_array().cloned().unwrap_or_default())
                .filter(is_managed)
                .count()
        })
    }

    #[test]
    fn install_twice_keeps_one_set() {
        let mut v = json!({});
        add_managed(&mut v, "C:\\x\\pet-hook.exe");
        add_managed(&mut v, "C:\\x\\pet-hook.exe");
        assert_eq!(managed_count(&v), EVENTS.len());
        assert_eq!(
            v["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"],
            60
        );
        assert!(
            v["hooks"]["PermissionRequest"][0]["hooks"][0]
                .get("async")
                .is_none()
        );
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["async"], true);
        assert_eq!(v["hooks"]["PreToolUse"][0]["matcher"], "*");
        assert!(v["hooks"]["Stop"][0].get("matcher").is_none());
        // 質問: `*` の非同期登録とは別の組で、同期・300 秒・`--ask` 付き（2 回登録しても 1 組）
        let ask = &v["hooks"]["PreToolUse"][1];
        assert_eq!(ask["matcher"], "AskUserQuestion");
        assert_eq!(ask["hooks"][0]["timeout"], 300);
        assert!(ask["hooks"][0].get("async").is_none());
        assert_eq!(ask["hooks"][0]["args"], json!([MARKER, ASK_ARG]));
        assert_eq!(
            v["hooks"]["PreToolUse"][0]["hooks"][0]["args"],
            json!([MARKER])
        );
        assert_eq!(v["hooks"]["PreToolUse"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn foreign_hooks_and_keys_survive() {
        let mut v = json!({
            "model": "opus",
            "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": "node other.mjs" } ] } ] }
        });
        let before = v.clone();
        add_managed(&mut v, "pet-hook.exe");
        assert_eq!(v["model"], "opus");
        assert_eq!(v["hooks"]["Stop"][0], before["hooks"]["Stop"][0]);
        remove_managed(&mut v);
        assert_eq!(v, before);
    }

    #[test]
    fn uninstall_from_scratch_removes_hooks_key() {
        let mut v = json!({ "model": "opus" });
        add_managed(&mut v, "pet-hook.exe");
        remove_managed(&mut v);
        assert_eq!(v, json!({ "model": "opus" }));
    }

    #[test]
    fn uninstall_keeps_preexisting_empty_entries() {
        let original = json!({
            "model": "opus",
            "hooks": { "Stop": [ { "matcher": "*", "hooks": [] } ], "PreToolUse": [] }
        });
        let mut v = original.clone();
        remove_managed(&mut v);
        assert_eq!(v, original);
        let mut v = json!({ "hooks": {} });
        remove_managed(&mut v);
        assert_eq!(v, json!({ "hooks": {} }));
        // 足してから外すと、元の空の組まで含めて元どおり（空の配列だけの hook 名は、足した時点で
        // 空でなくなるので戻らない。実害は無いので許す）
        let original = json!({ "hooks": { "Stop": [ { "matcher": "*", "hooks": [] } ] } });
        let mut v = original.clone();
        add_managed(&mut v, "pet-hook.exe");
        remove_managed(&mut v);
        assert_eq!(v, original);
    }

    #[test]
    fn marker_in_command_string_also_counts() {
        let mut v = json!({ "hooks": { "Stop": [ { "hooks": [
            { "type": "command", "command": "pet-hook.exe --pet-claude-managed" },
            { "type": "command", "command": "keep.exe" }
        ] } ] } });
        remove_managed(&mut v);
        assert_eq!(
            v["hooks"]["Stop"][0]["hooks"].as_array().map(Vec::len),
            Some(1)
        );
    }
}
