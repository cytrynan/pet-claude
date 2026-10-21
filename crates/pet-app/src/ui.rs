//! ペットの周りの UI（Tauri の WebView 窓）: クリック時のバー・チャット入力・右クリックメニュー・設定。
//!
//! - バーとメニューは `focusable(false)`＝作業中の窓からフォーカスを奪わない。
//!   フォーカスが無いので「外をクリックしたら閉じる」は低レベルのマウスフック（開いている間だけ）で拾う
//! - チャット入力は文字を打つので、フォーカスを取る（利用者が自分でチャットを選んだとき）
//! - **窓は使うときに作り、全部が隠れて `IDLE_CLOSE` たったら破棄する**。WebView2 は 1 枚でもあると
//!   ブラウザ本体などのプロセスが常駐し、4 枚を常に持つと専有メモリが 335MB になった（2026-10-21 実測）。
//!   普段はペット本体だけにする
//! - 窓を作る・出す処理は必ず `defer`（別スレッド経由でイベントループのキューに積む）で後回しにする。
//!   同期コマンド（WebView2 のコールバック）やペットのウィンドウプロシージャの中で WebView 窓を作ると、
//!   WebView2 の生成を待つ入れ子のメッセージループが走り、借用中の状態や再入とぶつかる
//! - すべてメインスレッドで動く。ペット側が `popups_visible()` を毎フレーム読んで
//!   「開いている間は散歩も睡眠もしない」を決める

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use pet_core::geometry::{Point, Rect};
use pet_core::placement;
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HHOOK, MSLLHOOKSTRUCT, SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE_LL,
    WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN,
};

use crate::log::log;
use crate::monitors;

pub const BAR: &str = "bar";
pub const CHAT: &str = "chat";
pub const MENU: &str = "menu";
pub const SETTINGS: &str = "settings";
const POPUPS: [&str; 3] = [BAR, CHAT, MENU];

/// 島の大きさ（論理ピクセル）。窓は影のために四方 12px 大きい。DESIGN.md と合わせる。
const SHADOW_PAD: f64 = 12.0;
const BAR_ISLAND: (f64, f64) = (96.0, 48.0);
const CHAT_ISLAND: (f64, f64) = (360.0, 48.0);
const MENU_ISLAND: (f64, f64) = (200.0, 150.0);
/// ペットの足元とバーの上端の重なり（論理ピクセル）。ペットの下端の透明な余白を詰める。
const BAR_OVERLAP: f64 = 6.0;
/// 全部の窓が隠れてからこれだけたったら破棄する（すぐ使い直すときは作り直さずに済む）。
const IDLE_CLOSE: Duration = Duration::from_secs(60);

/// WebView2 の起動引数（全窓で同じにする。違うと別の環境として扱われる）。
/// 描画プロセスを 1 つにまとめ、GPU プロセスを止める（専有メモリ 335→160MB・見た目は同じ。2026-10-21 実測）。
const DEFAULT_BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
     --renderer-process-limit=1 --disable-gpu";

thread_local! {
    static APP: RefCell<Option<AppHandle>> = const { RefCell::new(None) };
    static HOOK: Cell<Option<HHOOK>> = const { Cell::new(None) };
    /// 「内側」とみなす矩形（物理ピクセル）: 開いている島とペット。外のクリックで閉じる
    static INSIDE: RefCell<Vec<Rect>> = const { RefCell::new(Vec::new()) };
    /// 全部の窓が隠れた時刻（破棄の判定用）
    static HIDDEN_SINCE: Cell<Option<Instant>> = const { Cell::new(None) };
    /// いま出している窓。出した・隠したは自分で記録する（毎フレームの問い合わせを減らす）。
    /// 「`hide()` 直後の `is_visible()` が古い値を返す」は当初の推測で、確かめていない
    static SHOWN: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

fn app() -> Option<AppHandle> {
    APP.with(|a| a.borrow().clone())
}

/// `PET_WV_ARGS` で上書きできる（メモリの比較実験用。`scripts/verify/measure-memory.ps1`）。
fn browser_args() -> String {
    std::env::var("PET_WV_ARGS").unwrap_or_else(|_| DEFAULT_BROWSER_ARGS.to_string())
}

/// イベントループに回して後で実行する。ウィンドウプロシージャの借用中・WebView2 のコールバック中・
/// マウスフックの中で呼ばれる処理は必ずこれを通す。
///
/// `AppHandle::run_on_main_thread` は**メインスレッドから呼ぶとその場で同期実行する**
/// （tauri-runtime-wry 2.12.2 `send_user_message`: 呼び出し元がメインスレッドならキューに積まない）。
/// 後回しにするには別スレッドから呼んでイベントループのキューに積む（2026-10-21 検証係の指摘で判明）。
pub fn defer(f: impl FnOnce() + Send + 'static) {
    if let Some(handle) = app() {
        // spawn は失敗すると panic する（フックの中なら abort）。Builder で失敗をログに落とす
        let spawned = std::thread::Builder::new().spawn(move || {
            let _ = handle.run_on_main_thread(f);
        });
        if let Err(e) = spawned {
            log(format!("defer のスレッドを作れない: {e}"));
        }
    }
}

/// 起動時に 1 回。窓はまだ作らない。
pub fn init(app: &AppHandle) {
    APP.with(|a| *a.borrow_mut() = Some(app.clone()));
}

fn window(label: &str) -> Option<WebviewWindow> {
    app().and_then(|a| a.get_webview_window(label))
}

/// 島の窓を（無ければ）作って返す。
fn island(label: &str) -> Option<WebviewWindow> {
    if let Some(w) = window(label) {
        return Some(w);
    }
    let app = app()?;
    let (page, size, focusable) = match label {
        BAR => ("bar.html", BAR_ISLAND, false),
        CHAT => ("chat.html", CHAT_ISLAND, true),
        _ => ("menu.html", MENU_ISLAND, false),
    };
    let built = WebviewWindowBuilder::new(&app, label, WebviewUrl::App(page.into()))
        .title("pet-claude")
        .inner_size(size.0 + SHADOW_PAD * 2.0, size.1 + SHADOW_PAD * 2.0)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(focusable)
        .visible(false)
        .additional_browser_args(&browser_args())
        .build();
    let w = match built {
        Ok(w) => w,
        Err(e) => {
            log(format!("{label} の窓を作れない: {e}"));
            return None;
        }
    };
    if label == CHAT {
        // 入力欄からフォーカスが外れたら閉じる
        w.on_window_event(|e| {
            if let WindowEvent::Focused(false) = e {
                defer(|| hide(CHAT));
            }
        });
    }
    log(format!("{label} window created"));
    Some(w)
}

fn scale(dpi: u32) -> f64 {
    f64::from(dpi) / 96.0
}

/// 物理ピクセルの窓の大きさ（影の余白込み）。
fn window_size(island: (f64, f64), dpi: u32) -> (i32, i32) {
    let s = scale(dpi);
    (
        ((island.0 + SHADOW_PAD * 2.0) * s).round() as i32,
        ((island.1 + SHADOW_PAD * 2.0) * s).round() as i32,
    )
}

/// 窓を矩形の位置に置いて出す。位置を先に動かし（画面の DPI が変わればここで調整が入る）、大きさを後で決める。
fn mark_shown(label: &'static str, shown: bool) {
    SHOWN.with(|s| {
        let mut v = s.borrow_mut();
        v.retain(|l| *l != label);
        if shown {
            v.push(label);
        }
    });
}

fn place_and_show(label: &'static str, w: &WebviewWindow, rect: Rect, focus: bool) {
    let _ = w.set_position(PhysicalPosition::new(rect.left, rect.top));
    let _ = w.set_size(PhysicalSize::new(rect.width() as u32, rect.height() as u32));
    let _ = w.show();
    if focus {
        let _ = w.set_focus();
    }
    mark_shown(label, true);
    HIDDEN_SINCE.with(|h| h.set(None));
}

/// `pet` の真下・中央に `island` の窓を置く矩形（作業領域にクランプ）。
fn below_pet(pet: &Rect, island: (f64, f64), dpi: u32) -> Rect {
    let (w, h) = window_size(island, dpi);
    let s = scale(dpi);
    let top = pet.bottom - ((BAR_OVERLAP + SHADOW_PAD) * s).round() as i32;
    clamp_to_screen(Rect::from_xywh(pet.center().x - w / 2, top, w, h))
}

fn clamp_to_screen(rect: Rect) -> Rect {
    let ms = monitors::enumerate();
    match placement::monitor_for_rect(&ms, &rect) {
        Some(m) => placement::clamp_into(&rect, &m.work),
        None => rect,
    }
}

fn visible(label: &str) -> bool {
    SHOWN.with(|s| s.borrow().contains(&label))
}

/// バー・チャット・メニューのどれかが出ているか（ペットはこの間、散歩も睡眠もしない）。
pub fn popups_visible() -> bool {
    POPUPS.iter().any(|l| visible(l))
}

/// メニューのページが読み込まれたときに、今バーが出ているかを聞く（作った直後の eval は読み込み前で空振りする）。
pub fn bar_visible_for_menu() -> bool {
    bar_visible()
}

fn bar_visible() -> bool {
    visible(BAR)
}

/// 島の矩形（影の余白を除いた、見えている部分）。外クリック判定に使う。
fn island_rect(window_rect: Rect, dpi: u32) -> Rect {
    let pad = (SHADOW_PAD * scale(dpi)).round() as i32;
    Rect::new(
        window_rect.left + pad,
        window_rect.top + pad,
        window_rect.right - pad,
        window_rect.bottom - pad,
    )
}

fn window_rect(w: &WebviewWindow) -> Option<Rect> {
    let (p, s) = (w.outer_position().ok()?, w.outer_size().ok()?);
    Some(Rect::from_xywh(p.x, p.y, s.width as i32, s.height as i32))
}

fn set_inside(rects: Vec<Rect>) {
    INSIDE.with(|i| *i.borrow_mut() = rects);
    install_hook();
}

/// ペットのクリックでバーを出す／もう一度で隠す。ペットのウィンドウプロシージャから呼ぶので後で実行する。
pub fn toggle_bar(pet: Rect, dpi: u32) {
    defer(move || {
        if bar_visible() || visible(CHAT) {
            hide_all();
            return;
        }
        hide(MENU);
        let Some(w) = island(BAR) else { return };
        let rect = below_pet(&pet, BAR_ISLAND, dpi);
        place_and_show(BAR, &w, rect, false);
        set_inside(vec![pet, island_rect(rect, dpi)]);
        log("bar shown");
    });
}

/// バーの「チャット」: バーを隠してチャット入力を同じ場所に出す。
fn open_chat() {
    let Some(bar_rect) = window(BAR).and_then(|b| window_rect(&b)) else {
        return;
    };
    let dpi = monitors::at_point(bar_rect.center()).map_or(96, |m| m.dpi);
    let Some(chat) = island(CHAT) else { return };
    let (w, h) = window_size(CHAT_ISLAND, dpi);
    let c = bar_rect.center();
    let rect = clamp_to_screen(Rect::from_xywh(c.x - w / 2, c.y - h / 2, w, h));
    if let Some(bar) = window(BAR) {
        let _ = bar.hide();
    }
    mark_shown(BAR, false);
    place_and_show(CHAT, &chat, rect, true);
    let _ = chat.eval("window.focusInput && window.focusInput()");
    INSIDE.with(|i| {
        let mut v = i.borrow_mut();
        v.retain(|r| *r != island_rect(bar_rect, dpi));
        v.push(island_rect(rect, dpi));
    });
    log("chat shown");
}

/// 右クリックメニューを出す。ペットのウィンドウプロシージャから呼ぶので後で実行する。
pub fn show_menu(at: Point, pet: Rect, dpi: u32) {
    defer(move || {
        let bar_shown = bar_visible();
        let Some(w) = island(MENU) else { return };
        let (mw, mh) = window_size(MENU_ISLAND, dpi);
        let pad = (SHADOW_PAD * scale(dpi)).round() as i32;
        // ペットに重ねない（ペットは 1 秒ごとに最前面を取り直すので、重なるとメニューが下敷きになる）。
        // 島の左端をペットの右端に合わせ、高さはカーソルに合わせる。右に入らなければペットの左へ
        let mut rect = Rect::from_xywh(pet.right - pad, at.y - pad, mw, mh);
        if let Some(m) = monitors::at_point(pet.center())
            && rect.right > m.work.right
        {
            rect = Rect::from_xywh(pet.left - mw + pad, at.y - pad, mw, mh);
        }
        let rect = clamp_to_screen(rect);
        let _ = w.eval(format!(
            "window.setBarShown && window.setBarShown({bar_shown})"
        ));
        place_and_show(MENU, &w, rect, false);
        // 出ているバー・チャットも内側に入れる（入れないと、チャットの中のクリックで全部閉じる）
        let mut inside = vec![pet, island_rect(rect, dpi)];
        for label in [BAR, CHAT] {
            if visible(label)
                && let Some(r) = window(label).and_then(|w| window_rect(&w))
            {
                inside.push(island_rect(r, dpi));
            }
        }
        set_inside(inside);
        log("menu shown");
    });
}

fn after_hide() {
    if popups_visible() {
        return;
    }
    uninstall_hook();
    if !visible(SETTINGS) {
        schedule_close();
    }
}

pub fn hide(label: &'static str) {
    if let Some(w) = window(label) {
        let _ = w.hide();
    }
    mark_shown(label, false);
    after_hide();
}

pub fn hide_all() {
    log("popups hidden (all)");
    for l in POPUPS {
        if let Some(w) = window(l) {
            let _ = w.hide();
        }
        mark_shown(l, false);
    }
    after_hide();
}

/// 全部の窓が隠れてから `IDLE_CLOSE` たったら破棄する（WebView2 のプロセスを畳む）。
fn schedule_close() {
    let since = Instant::now();
    HIDDEN_SINCE.with(|h| h.set(Some(since)));
    // AppHandle はメインスレッドの thread_local にあるので、待つスレッドへは複製を持たせる
    // （別スレッドから defer を呼ぶと thread_local が空で何もしない＝2026-10-21 に一度踏んだ）
    let Some(handle) = app() else { return };
    std::thread::spawn(move || {
        std::thread::sleep(IDLE_CLOSE);
        let _ = handle.run_on_main_thread(move || {
            let still = HIDDEN_SINCE.with(Cell::get) == Some(since);
            if !still || popups_visible() || visible(SETTINGS) {
                return;
            }
            let Some(app) = app() else { return };
            let mut n = 0;
            for (_, w) in app.webview_windows() {
                let _ = w.destroy();
                n += 1;
            }
            SHOWN.with(|s| s.borrow_mut().clear());
            log(format!("idle: destroyed {n} UI windows"));
        });
    });
}

/// 設定の窓（普通の窓）。あれば前に出す。後で実行する（WebView2 のコールバック中に作らない）。
pub fn open_settings() {
    defer(|| {
        if let Some(w) = window(SETTINGS) {
            let _ = w.eval("window.reload && window.reload()");
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            mark_shown(SETTINGS, true);
            return;
        }
        let Some(app) = app() else { return };
        let built =
            WebviewWindowBuilder::new(&app, SETTINGS, WebviewUrl::App("settings.html".into()))
                .title("pet-claude の設定")
                .inner_size(440.0, 330.0)
                .resizable(false)
                .maximizable(false)
                .center()
                .additional_browser_args(&browser_args())
                .build();
        match built {
            Ok(w) => {
                // 閉じたら（破棄されたら）、他の窓も隠れていれば破棄を予約する
                w.on_window_event(|e| {
                    if let WindowEvent::Destroyed = e {
                        defer(|| {
                            mark_shown(SETTINGS, false);
                            after_hide();
                        });
                    }
                });
                let _ = w.set_focus();
                mark_shown(SETTINGS, true);
                HIDDEN_SINCE.with(|h| h.set(None));
                log("settings window created");
            }
            Err(e) => log(format!("設定の窓を開けない: {e}")),
        }
    });
}

/// UI からの操作。`invoke("ui_action", { action })` で呼ばれる（WebView2 のコールバック中）ので、
/// 窓を作ったり閉じたりする処理はすべて後で実行する。
pub fn action(action: String) {
    log(format!("ui action: {action}"));
    defer(move || match action.as_str() {
        "chat" => open_chat(),
        // 音声入力は Phase 3。いまはバーを閉じるだけ
        "voice" => hide(BAR),
        "close" => hide_all(),
        "menu:settings" => {
            hide(MENU);
            open_settings();
        }
        "menu:hide-bar" => hide_all(),
        "menu:reset" => {
            hide_all();
            crate::pet_window::reset_position();
        }
        "menu:quit" => {
            crate::pet_window::shutdown();
            log("quit");
            if let Some(app) = app() {
                app.exit(0);
            }
        }
        _ => {}
    });
}

// ---- 外側クリックの検出（低レベルのマウスフック。島が出ている間だけ入れる） ----

unsafe extern "system" fn mouse_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let msg = wp.0 as u32;
        if msg == WM_LBUTTONDOWN || msg == WM_RBUTTONDOWN || msg == WM_MBUTTONDOWN {
            // SAFETY: WH_MOUSE_LL の lParam は MSLLHOOKSTRUCT を指す
            let info = unsafe { &*(lp.0 as *const MSLLHOOKSTRUCT) };
            let p = Point {
                x: info.pt.x,
                y: info.pt.y,
            };
            let outside = INSIDE.with(|i| i.borrow().iter().all(|r| !r.contains(p)));
            if std::env::var_os("PET_TRACE").is_some() {
                let rects = INSIDE.with(|i| format!("{:?}", i.borrow()));
                log(format!("hook down at ({},{}) inside={rects}", p.x, p.y));
            }
            if outside {
                log(format!("outside click at ({},{}) -> close", p.x, p.y));
                // フックの中では重い処理をしない。閉じるのはイベントループに回す
                defer(hide_all);
            }
        }
    }
    // SAFETY: 次のフックへ渡す
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

fn install_hook() {
    if HOOK.with(Cell::get).is_some() {
        return;
    }
    // SAFETY: 自分のモジュールのフック関数を、このスレッドのメッセージループで受ける
    let hook = unsafe {
        GetModuleHandleW(None)
            .and_then(|m| SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(m.into()), 0))
    };
    match hook {
        Ok(h) => HOOK.with(|c| c.set(Some(h))),
        Err(e) => log(format!("マウスフックを入れられない: {e}")),
    }
}

fn uninstall_hook() {
    if let Some(h) = HOOK.with(Cell::take) {
        // SAFETY: install_hook で得たハンドル
        let _ = unsafe { UnhookWindowsHookEx(h) };
    }
}
