//! pet-claude: Claude Code 用の Windows デスクトップペット。
//!
//! Phase 0: ペットの窓（透過・最前面・フォーカス非取得・不透明部分だけのクリック・
//! ドラッグ・画面外への逸脱防止）とトレイ。Claude Code 連携は Phase 2。
//!
//! 引数:
//! - `--probe`        ディスプレイ構成をログと標準出力に書いて終了
//! - `--pet <dir>`    `pet.json` と スプライトシートのあるフォルダを使う（既定は内蔵の仮画像）
//! - `--install-hooks [--settings <path>]`   Claude Code の settings.json に pet-hook を登録して終了
//! - `--uninstall-hooks [--settings <path>]` 登録を外して終了（既定は `~/.claude/settings.json`）
//! - `--install-autostart [--task-name <名前>] [--disabled]` ログオンで起こす Windows タスクを登録して終了（Phase 4）
//! - `--uninstall-autostart [--task-name <名前>]` / `--autostart-status [--task-name <名前>]` 解除・状態を表示して終了

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod chat;
mod clipstore;
mod cred;
mod install;
mod ipc_server;
mod log;
mod monitors;
mod pet_window;
mod pets;
mod state;
mod ui;

use std::path::Path;

use tauri::RunEvent;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::core::w;

use crate::log::log;

/// 起動時に表示するペット。`--pet <dir>` が最優先、次に state.json で選んだもの、無ければ内蔵の仮キャラ。
/// 選んだペットが消えていた・読めないときも仮キャラで起動する（起動できないよりよい）。
fn startup_pet(
    pet_dir: Option<&str>,
    state: &state::AppState,
) -> Result<(pets::PetEntry, pets::LoadedSheet), String> {
    let need = pets::needed_cell(pet_window::display_factor(state.scale));
    if let Some(dir) = pet_dir {
        let entry = pets::entry_for_dir(Path::new(dir))?;
        let loaded = pets::load(&entry, need)?;
        return Ok((entry, loaded));
    }
    // 試す順: state.json で選んだペット → フォルダにあるペット（見つかった順）→ 仮キャラ。読めないものは
    // 飛ばして次を試す（壊れたペットが 1 つ先にあるだけで仮キャラになっていた。2026-10-21 故障注入で発見）
    let all = pets::discover();
    let mut last_err = String::from("ペットが 1 つも無い");
    for entry in pets::startup_order(&all, state.pet.as_deref()) {
        match pets::load(entry, need) {
            Ok(loaded) => return Ok((entry.clone(), loaded)),
            Err(e) => {
                log(format!("ペット {} を読めないので次を試す: {e}", entry.id));
                last_err = e;
            }
        }
    }
    Err(last_err)
}

/// 試験用（デバッグビルドで、置き場所を `PET_CLAUDE_DATA_DIR` で差し替えた時だけ）:
/// `PET_TEST_SWITCH_PET=<id>[,<id>...]` なら 2 秒後に、設定画面と同じ関数でペットを切り替える。
/// 本物のマウスで設定画面を操作せずに、切り替えの経路を確かめるため。
fn switch_pet_for_tests(app: tauri::AppHandle) {
    if !cfg!(debug_assertions) || std::env::var_os("PET_CLAUDE_DATA_DIR").is_none() {
        return;
    }
    let Ok(id) = std::env::var("PET_TEST_SWITCH_PET") else {
        return;
    };
    // カンマで区切ると続けて選ぶ（読み込み中に選び直しても、最後に選んだものが表示されるかを見る）
    let _ = std::thread::Builder::new().spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let _ = app.run_on_main_thread(move || {
            for one in id.split(',') {
                pet_window::set_pet(one.to_string());
            }
        });
    });
}

/// 試験用（`switch_pet_for_tests` と同じ条件）: `PET_TEST_CHAT` の手順を 2 秒後から順に行う。
/// 手順は `|` で区切り、`send:<文>`（チャット欄の送信と同じ関数）・`new`（新しいチャット）・`wait:<ミリ秒>`・
/// `ui:<操作>`（バー・吹き出しのボタンと同じ `ui::action`）・`auth`（設定画面の「確かめる」と同じ関数）。
/// 本物のマウスとキーボードを使わずに、チャットの経路（時間切れ・止める・続き）を確かめるため。
fn chat_for_tests(app: tauri::AppHandle) {
    if !cfg!(debug_assertions) || std::env::var_os("PET_CLAUDE_DATA_DIR").is_none() {
        return;
    }
    let Ok(steps) = std::env::var("PET_TEST_CHAT") else {
        return;
    };
    let _ = std::thread::Builder::new().spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        for step in steps.split('|') {
            if let Some(ms) = step.strip_prefix("wait:") {
                let ms = ms.parse().unwrap_or(0);
                std::thread::sleep(std::time::Duration::from_millis(ms));
            } else if let Some(text) = step.strip_prefix("send:") {
                let (text, handle) = (text.to_string(), app.clone());
                let _ = app.run_on_main_thread(move || {
                    let reply = chat::submit(handle, text, pet_window::chat_auth());
                    log(format!("test chat: submit -> {reply}"));
                });
            } else if step == "new" {
                let _ = app.run_on_main_thread(ui::new_chat);
            } else if step == "auth" {
                // 設定画面の「確かめる」と同じ関数（このスレッドで待つ）
                let label = chat::auth_status(pet_core::chat::AuthMode::Subscription);
                log(format!("test chat: auth -> {label}"));
            } else if let Some(action) = step.strip_prefix("ui:") {
                // ボタンと同じ関数（例 `ui:chat:more`＝吹き出しの「続きを見る」）。撮影でマウスを使わないため
                // defer はメインスレッドの thread_local を読むので、メインスレッドから呼ぶ
                let action = action.to_string();
                let _ = app.run_on_main_thread(move || ui::action(action));
            }
        }
    });
}

/// バー・メニューのボタン。`invoke("ui_action", { action })`
#[tauri::command]
fn ui_action(action: String) {
    ui::action(action);
}

/// チャットの送信。待たずに戻り、答えは後で吹き出しとチャット欄に出す。戻り値は入力欄に出す一言
#[tauri::command]
fn chat_submit(app: tauri::AppHandle, text: String) -> String {
    chat::submit(app, text, pet_window::chat_auth())
}

/// チャット欄が読み込み時に聞く（最後の答えと待ち中か）
#[tauri::command]
fn chat_state() -> serde_json::Value {
    ui::chat_state()
}

/// 設定画面: チャットの接続（`subscription` / `api_key`）
#[tauri::command]
fn chat_auth_set(mode: String) {
    pet_window::set_chat_auth(pet_core::chat::AuthMode::from_state(Some(&mode)));
}

/// 設定画面: API キーを資格情報マネージャーに保存する。保存できたら true（鍵はログにも戻り値にも出さない）
#[tauri::command]
fn chat_key_save(key: String) -> bool {
    match cred::save_key(&key) {
        Ok(()) => {
            log("chat: API key saved");
            true
        }
        Err(e) => {
            log(format!("chat: API key not saved: {e}"));
            false
        }
    }
}

/// 設定画面: API キーを消す。消せたら（もともと無かったも含む）true
#[tauri::command]
fn chat_key_delete() -> bool {
    match cred::delete_key() {
        Ok(()) => {
            log("chat: API key deleted");
            true
        }
        Err(e) => {
            log(format!("chat: API key not deleted: {e}"));
            false
        }
    }
}

/// 設定画面の「確かめる」。`claude auth status` に数秒かかるので async（メインスレッドを止めない）
#[tauri::command]
async fn chat_auth_check(mode: String) -> String {
    let mode = pet_core::chat::AuthMode::from_state(Some(&mode));
    tauri::async_runtime::spawn_blocking(move || chat::auth_status(mode))
        .await
        .unwrap_or_else(|_| "確かめられませんでした".into())
}

/// メニューのページが読み込まれたときに呼ぶ。今バーが出ているか
#[tauri::command]
fn bar_shown() -> bool {
    ui::bar_visible_for_menu()
}

/// 吹き出しのページが読み込まれたときに呼ぶ。今出す中身
#[tauri::command]
fn bubble_state() -> serde_json::Value {
    ui::bubble_state()
}

/// 吹き出しの承認のボタン。`decision` は allow / deny / ask
#[tauri::command]
fn permission_decide(id: String, decision: String) {
    ui::permission_decide(id, decision);
}

/// 設定画面のペットの選択。`invoke("pet_set", { id })`
#[tauri::command]
fn pet_set(id: String) {
    pet_window::set_pet(id);
}

#[tauri::command]
fn settings_get() -> serde_json::Value {
    pet_window::settings()
}

#[tauri::command]
async fn autostart_get() -> serde_json::Value {
    tauri::async_runtime::spawn_blocking(autostart::status_json)
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn autostart_set(on: bool) -> serde_json::Value {
    tauri::async_runtime::spawn_blocking(move || autostart::set_json(on))
        .await
        .unwrap_or_default()
}

#[tauri::command]
fn settings_set(scale: f32, tts: bool, wake: bool, walk: bool, save: bool) {
    pet_window::set_settings(scale, tts, wake, walk, save);
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() {
    // SAFETY: プロセス開始直後に 1 回だけ呼ぶ。Tauri が後で同じ設定をしても害はない
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--probe") {
        let line = pet_window::describe_monitors(&monitors::enumerate());
        log(format!("probe: {line}"));
        println!("{line}");
        return;
    }

    let install = args.iter().any(|a| a == "--install-hooks");
    if install || args.iter().any(|a| a == "--uninstall-hooks") {
        let line = install::run(install, arg_value(&args, "--settings").as_deref())
            .unwrap_or_else(|e| format!("失敗: {e}"));
        log(format!("hooks: {line}"));
        println!("{line}");
        return;
    }

    if [
        "--install-autostart",
        "--uninstall-autostart",
        "--autostart-status",
    ]
    .iter()
    .any(|f| args.iter().any(|a| a == f))
    {
        let line = autostart::run_cli(&args);
        log(format!("autostart: {line}"));
        println!("{line}");
        return;
    }

    // 二重起動しない（名前付きミューテックス。プロセス終了で OS が解放する）。
    // 試験（デバッグビルドで置き場所を差し替えた時）は別の名前にし、普段のペットを止めずに並べて動かせる
    // （チャットの故障注入は何度も起動し直すので、K のペットを毎回止めずに済むように。2026-10-21）
    let name = if cfg!(debug_assertions) && std::env::var_os("PET_CLAUDE_DATA_DIR").is_some() {
        w!("Local\\pet-claude-single-instance-test")
    } else {
        w!("Local\\pet-claude-single-instance")
    };
    // SAFETY: 名前は 'static。ハンドルはプロセスの寿命まで保持する
    let _mutex = unsafe { CreateMutexW(None, false, name) };
    // SAFETY: 直前の API のエラーを読むだけ
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        log("既に起動しているので終了");
        return;
    }

    let open_settings = args.iter().any(|a| a == "--open-settings");
    let app_state = state::AppState::load();
    let (pet, sheet) = match startup_pet(arg_value(&args, "--pet").as_deref(), &app_state) {
        Ok(p) => p,
        Err(e) => {
            log(format!("スプライトを読めない: {e}"));
            return;
        }
    };
    log(format!("start {}", env!("CARGO_PKG_VERSION")));

    let result = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ui_action,
            bar_shown,
            chat_submit,
            chat_state,
            chat_auth_set,
            chat_key_save,
            chat_key_delete,
            chat_auth_check,
            settings_get,
            settings_set,
            autostart_get,
            autostart_set,
            bubble_state,
            permission_decide,
            pet_set
        ])
        .setup(move |app| {
            let settings = MenuItem::with_id(app, "settings", "設定", true, None::<&str>)?;
            let reset = MenuItem::with_id(app, "reset", "位置をリセット", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings, &reset, &quit])?;
            let mut tray = TrayIconBuilder::with_id("pet")
                .tooltip("pet-claude")
                .menu(&menu);
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.on_menu_event(|app, event| match event.id.as_ref() {
                "settings" => ui::open_settings(),
                "reset" => pet_window::reset_position(),
                "quit" => {
                    chat::shutdown();
                    pet_window::shutdown();
                    ipc_server::stop();
                    log("quit");
                    app.exit(0);
                }
                _ => {}
            })
            .build(app)?;
            ui::init(app.handle());
            pet_window::create(sheet, pet, app_state, app.handle().clone())?;
            ipc_server::start(app.handle().clone());
            if open_settings {
                ui::open_settings();
            }
            switch_pet_for_tests(app.handle().clone());
            chat_for_tests(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!());
    match result {
        // UI の窓はすぐ破棄するので、「最後の窓が閉じたら終了」を止める。
        // 終了コード付き（メニュー・トレイの「終了」＝ app.exit(0)）だけを通す
        Ok(app) => app.run(|_, event| {
            if let RunEvent::ExitRequested { api, code, .. } = event
                && code.is_none()
            {
                api.prevent_exit();
            }
        }),
        Err(e) => log(format!("tauri error: {e}")),
    }
}
