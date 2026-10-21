//! pet-claude: Claude Code 用の Windows デスクトップペット。
//!
//! Phase 0: ペットの窓（透過・最前面・フォーカス非取得・不透明部分だけのクリック・
//! ドラッグ・画面外への逸脱防止）とトレイ。Claude Code 連携は Phase 2。
//!
//! 引数:
//! - `--probe`        ディスプレイ構成をログと標準出力に書いて終了
//! - `--pet <dir>`    `pet.json` と スプライトシートのあるフォルダを使う（既定は内蔵の仮画像）

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod log;
mod monitors;
mod pet_window;
mod state;
mod ui;

use std::path::Path;

use image::RgbaImage;
use pet_core::sprite::PetManifest;
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

const PLACEHOLDER_SHEET: &[u8] = include_bytes!("../../../assets/pets/placeholder/spritesheet.png");

fn load_sheet(pet_dir: Option<&str>) -> Result<RgbaImage, String> {
    let Some(dir) = pet_dir else {
        return image::load_from_memory(PLACEHOLDER_SHEET)
            .map(|i| i.to_rgba8())
            .map_err(|e| e.to_string());
    };
    let dir = Path::new(dir);
    let manifest: PetManifest = std::fs::read_to_string(dir.join("pet.json"))
        .map_err(|e| format!("pet.json: {e}"))
        .and_then(|t| serde_json::from_str(&t).map_err(|e| format!("pet.json: {e}")))?;
    let img = image::open(dir.join(&manifest.spritesheet_path))
        .map_err(|e| format!("{}: {e}", manifest.spritesheet_path))?;
    log(format!(
        "pet '{}' ({}) loaded",
        manifest.display_name, manifest.id
    ));
    Ok(img.to_rgba8())
}

/// バー・メニューのボタン。`invoke("ui_action", { action })`
#[tauri::command]
fn ui_action(action: String) {
    ui::action(action);
}

/// チャットの送信。Claude へ送るのは Phase 3。いまはログに残し、入力欄に返す一言を返す
#[tauri::command]
fn chat_submit(text: String) -> String {
    log(format!("chat submit ({} 字)", text.chars().count()));
    "送信は Phase 3 で使えるようになります".to_string()
}

/// メニューのページが読み込まれたときに呼ぶ。今バーが出ているか
#[tauri::command]
fn bar_shown() -> bool {
    ui::bar_visible_for_menu()
}

#[tauri::command]
fn settings_get() -> serde_json::Value {
    pet_window::settings()
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

    // 二重起動しない（名前付きミューテックス。プロセス終了で OS が解放する）
    // SAFETY: 名前は 'static。ハンドルはプロセスの寿命まで保持する
    let _mutex = unsafe { CreateMutexW(None, false, w!("Local\\pet-claude-single-instance")) };
    // SAFETY: 直前の API のエラーを読むだけ
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        log("既に起動しているので終了");
        return;
    }

    let sheet = match load_sheet(arg_value(&args, "--pet").as_deref()) {
        Ok(s) => s,
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
            settings_get,
            settings_set
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
                    pet_window::shutdown();
                    log("quit");
                    app.exit(0);
                }
                _ => {}
            })
            .build(app)?;
            ui::init(app.handle());
            pet_window::create(sheet, state::AppState::load())?;
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
