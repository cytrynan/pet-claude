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
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use pet_core::geometry::{Point, Rect};
use pet_core::ipc::{PermissionState, WireEvent};
use pet_core::placement;
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetCursorPos, HHOOK, MSLLHOOKSTRUCT, SetWindowsHookExW, UnhookWindowsHookEx,
    WH_MOUSE_LL, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_MOUSEMOVE, WM_RBUTTONDOWN,
};

use crate::log::log;
use crate::monitors;

pub const BAR: &str = "bar";
pub const CHAT: &str = "chat";
pub const MENU: &str = "menu";
pub const SETTINGS: &str = "settings";
pub const BUBBLE: &str = "bubble";
const POPUPS: [&str; 3] = [BAR, CHAT, MENU];

/// 島の大きさ（論理ピクセル）。窓は影のために四方 12px 大きい。DESIGN.md と合わせる。
const SHADOW_PAD: f64 = 12.0;
const BAR_ISLAND: (f64, f64) = (96.0, 48.0);
const CHAT_ISLAND: (f64, f64) = (360.0, 48.0);
const MENU_ISLAND: (f64, f64) = (200.0, 150.0);
/// 吹き出し: 固定文言のカプセルと、承認のカード（DESIGN.md）。
const STATUS_ISLAND: (f64, f64) = (180.0, 40.0);
const PERMISSION_ISLAND: (f64, f64) = (320.0, 148.0);
/// 「許可」を出さないカードは注記が 1 行増える（撮影で要約の行が潰れたので分けた）。
const PERMISSION_NOTE_ISLAND: (f64, f64) = (320.0, 172.0);
/// チャットの答えの吹き出し: 幅と、1 行目の高さ（上下の余白込み）・2 行目からの 1 行の高さ・「続きを見る」の行。
/// 本文 13px・行の高さ 20px。幅 300px の中身 272px＝全角 20 字。字幅の誤差を見込んで 1 行 38 桁で数える。
const ANSWER_WIDTH: f64 = 300.0;
const ANSWER_BASE: f64 = 44.0;
const ANSWER_LINE_HEIGHT: f64 = 20.0;
const ANSWER_MORE_ROW: f64 = 34.0;
const ANSWER_COLS: usize = 38;
/// 答えの吹き出しを出しておく時間: 基本＋1 字ごと、上限あり（読み終える前に消えないように）。
const ANSWER_BASE_MS: u64 = 6000;
const ANSWER_PER_CHAR_MS: u64 = 120;
const ANSWER_MAX_MS: u64 = 20_000;
/// チャット欄の上に出す答えの欄の高さ（論理ピクセル。中はスクロール）と、入力欄との間。
const CHAT_ANSWER_HEIGHT: f64 = 188.0;
const CHAT_GAP: f64 = 8.0;
/// チャット欄を出してから、前面を失っても閉じずに取り直す間（実機では出して 7ms で失った）。
/// 人が外を押すにはこれより長くかかる（ダブルクリックの間隔 500ms 程度）ので、その操作は閉じる側に入る。
const CHAT_FOCUS_GRACE: Duration = Duration::from_millis(600);
/// 失敗の理由が吹き出しの検査に掛かった（パスなどを含む）ときの吹き出しの文。理由はチャット欄に出す。
const FAILURE_HIDDEN: &str = "うまくいきませんでした。理由はチャット欄に";
/// 吹き出しの下端を、ペットの窓の上端からこの分だけ下げる（素材の上の透明な余白を詰める。論理ピクセル）。
const BUBBLE_DROP: f64 = 20.0;
/// 承認の要約: 折り返して全文を出す。行数の上限・1 行の桁数（多めに数える）・1 行の高さ（論理ピクセル）。
/// 上限を超える要約には「許可」を出さない（4 行＝半角 144 桁で、要約の上限 120 字は半角なら収まる）。
const MAX_SUMMARY_LINES: usize = 4;
const SUMMARY_COLS: usize = 36;
const SUMMARY_LINE_HEIGHT: f64 = 17.0;
/// hook が「要約は元の文字列と同じ」と確かめたときに `detail` に付ける印（pet-hook と同じ値）。
const EXACT: &str = "exact";
/// 出たばかりの承認のカードを押せない時間（ダブルクリックの間隔 500ms より長く）。
const FRESH_CARD_GUARD: Duration = Duration::from_millis(600);
/// ホバーで出したバーから離れて閉じるまでの猶予（縁をかすめただけで閉じないように）。
const HOVER_LEAVE_MS: u64 = 400;
/// 固定文言を出しておく時間。
const STATUS_MS: u64 = 3000;
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
    /// ペットの窓の矩形と DPI（吹き出しの位置合わせ用。ペットが描くたびに更新）
    static PET_AT: Cell<Option<(Rect, u32)>> = const { Cell::new(None) };
    /// 答えを待っている承認（古い順）。先頭を吹き出しに出す
    static PERMS: RefCell<VecDeque<(String, WireEvent)>> = const { RefCell::new(VecDeque::new()) };
    /// 吹き出しに今出している中身（bubble.html が読み込み時に bubble_state で聞く）
    static BUBBLE_STATE: RefCell<serde_json::Value> = const { RefCell::new(serde_json::Value::Null) };
    /// 固定文言の世代（新しい文言が出たら古いタイマーは何もしない）
    static STATUS_GEN: Cell<u64> = const { Cell::new(0) };
    /// 承認のカードの見回り（期限切れを下げる）が動いているか
    static PRUNING: Cell<bool> = const { Cell::new(false) };
    /// 承認のカードの先頭と、それが先頭になった時刻（出たばかりのカードへの押下を捨てる）
    static FRONT_SINCE: RefCell<Option<(String, Instant)>> = const { RefCell::new(None) };
    /// いま出ているバーはカーソルが乗って出したもの（離れたら閉じる）。クリック・チャット・右クリックで false
    static HOVER_BAR: Cell<bool> = const { Cell::new(false) };
    /// ホバーのバーから離れたかの確認を待っている
    static LEAVE_PENDING: Cell<bool> = const { Cell::new(false) };
    /// チャットの最後の答え（チャット欄に出す全文, 失敗か）。「新しいチャット」で消す
    static CHAT_LAST: RefCell<Option<(String, bool)>> = const { RefCell::new(None) };
    /// チャット欄を開いた時のペットの矩形（物理座標）と DPI。チャット欄はこれに重ならないよう置く
    static CHAT_PET: Cell<Option<(Rect, u32)>> = const { Cell::new(None) };
    /// チャット欄を開いた時刻。開いた直後の前面の喪失を 1 回だけ取り直すのに使う（使ったら消す）
    static CHAT_OPENED_AT: Cell<Option<Instant>> = const { Cell::new(None) };
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
        BUBBLE => ("bubble.html", STATUS_ISLAND, false),
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
        // 入力欄からフォーカスが外れたら閉じる。ただし出した直後（CHAT_FOCUS_GRACE 以内）の 1 回は、
        // 閉じずに前面を取り直す。窓を作った直後の 1 回目は 7ms で前面を失って閉じ、K がもう一度
        // 押す必要があった（2026-10-21 K の実機確認）。取り直しは 1 回だけ（本当に外を押した時と取り合わない）
        w.on_window_event(|e| {
            if let WindowEvent::Focused(false) = e {
                defer(|| {
                    let fresh = CHAT_OPENED_AT
                        .with(|t| t.take())
                        .is_some_and(|at| at.elapsed() < CHAT_FOCUS_GRACE);
                    if fresh
                        && visible(CHAT)
                        && let Some(w) = window(CHAT)
                    {
                        log("chat lost focus right after show -> refocus");
                        let _ = w.set_focus();
                        let _ = w.eval("window.focusInput && window.focusInput()");
                        return;
                    }
                    // 閉じた理由を残す（前面を取れずに開いた直後に閉じたのか、を見分けるため）
                    log("chat hidden: focus lost");
                    hide(CHAT);
                });
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

/// `pet` の真下・中央に `island` の窓を置く矩形（作業領域にクランプ）。下に入りきらなければ真上に置く。
fn below_pet(pet: &Rect, island: (f64, f64), dpi: u32) -> Rect {
    let (w, h) = window_size(island, dpi);
    let overlap = ((BAR_OVERLAP + SHADOW_PAD) * scale(dpi)).round() as i32;
    let work = monitors::at_point(pet.center()).map(|m| m.work);
    let rect = match work {
        Some(work) => under_or_over(pet, (w, h), overlap, &work),
        None => Rect::from_xywh(pet.center().x - w / 2, pet.bottom - overlap, w, h),
    };
    clamp_to_screen(rect)
}

/// ペットの真下（下端から `overlap` だけ重ねる）に `size` の窓を置く。作業領域の下に入りきらないときは
/// 真上（上端から `overlap` だけ重ねる）に置く。画面の下端にいるペットで、下に置いたバーを作業領域の内側へ
/// 押し戻すとペットと重なり、ペットが 1 秒ごとに最前面を取り直すのでバーが隠れた（2026-10-21 撮影で発見）。
fn under_or_over(pet: &Rect, size: (i32, i32), overlap: i32, work: &Rect) -> Rect {
    let (w, h) = size;
    let left = pet.center().x - w / 2;
    let under = Rect::from_xywh(left, pet.bottom - overlap, w, h);
    if under.bottom <= work.bottom {
        under
    } else {
        Rect::from_xywh(left, pet.top + overlap - h, w, h)
    }
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
        // カーソルが乗って出たバーをクリックしたら、閉じずにそのまま固定する（乗った直後のクリックで
        // バーが消えないように。固定したバーは、もう一度のクリックか外側のクリックで閉じる）
        if HOVER_BAR.with(Cell::get) && bar_visible() {
            HOVER_BAR.with(|h| h.set(false));
            log("bar pinned by click");
            return;
        }
        if bar_visible() || visible(CHAT) {
            hide_all();
            return;
        }
        HOVER_BAR.with(|h| h.set(false));
        open_bar(pet, dpi);
    });
}

/// バーをペットの真下に出す（クリックでもカーソルが乗ったときでも同じ）。
fn open_bar(pet: Rect, dpi: u32) {
    hide(MENU);
    let Some(w) = island(BAR) else { return };
    let rect = below_pet(&pet, BAR_ISLAND, dpi);
    place_and_show(BAR, &w, rect, false);
    set_inside(vec![pet, island_rect(rect, dpi)]);
    log("bar shown");
}

/// バーの「チャット」・吹き出しの「続きを見る」: バーを隠してチャット欄をペットの真下に出す
/// （下に収まらなければ真上。ペットには重ねない）。
fn open_chat() {
    // チャットを開いたら、カーソルが離れても閉じない（入力中に閉じると打った文字が消える）
    HOVER_BAR.with(|h| h.set(false));
    let Some(pet) = PET_AT.with(Cell::get) else {
        return;
    };
    CHAT_PET.with(|a| a.set(Some(pet)));
    let anchor = pet;
    let Some(chat) = island(CHAT) else { return };
    let rect = chat_rect();
    if let Some(bar) = window(BAR) {
        let _ = bar.hide();
    }
    mark_shown(BAR, false);
    CHAT_OPENED_AT.with(|t| t.set(Some(Instant::now())));
    place_and_show(CHAT, &chat, rect, true);
    let _ = chat.eval(format!(
        "window.renderChat && window.renderChat({})",
        chat_state()
    ));
    let _ = chat.eval("window.focusInput && window.focusInput()");
    set_chat_inside(rect, anchor.1);
    log("chat shown");
}

/// 外側クリックの判定に、チャット欄とペットを入れる。
fn set_chat_inside(rect: Rect, dpi: u32) {
    let mut inside = vec![island_rect(rect, dpi)];
    if let Some((pet, _)) = PET_AT.with(Cell::get) {
        inside.push(pet);
    }
    set_inside(inside);
}

/// チャット欄の島の大きさ。答えがあれば入力欄の上に答えの欄を足す。
fn chat_island() -> (f64, f64) {
    // 待ち中も答えの欄を出す（「新しいチャット」で取り消せるように。2026-10-21 検証係の指摘）
    let has_answer = CHAT_LAST.with(|c| c.borrow().is_some()) || crate::chat::busy();
    if has_answer {
        (CHAT_ISLAND.0, CHAT_ISLAND.1 + CHAT_GAP + CHAT_ANSWER_HEIGHT)
    } else {
        CHAT_ISLAND
    }
}

/// チャット欄の窓の矩形。バーと同じく、ペットの真下に置き、作業領域の下に収まらなければ真上に置く。
/// 答えの欄が足されて高くなったときも同じ規則で置き直す（入力欄の位置を保つ置き方だと、画面の下の方では
/// 答えの欄がペットに重なり、ペットに隠れた。2026-10-22 K の指示「端っこでチャットを開くと重なる問題も直して」）。
fn chat_rect() -> Rect {
    let Some((pet, dpi)) = CHAT_PET.with(Cell::get) else {
        return Rect::from_xywh(0, 0, 1, 1);
    };
    below_pet(&pet, chat_island(), dpi)
}

/// chat.html が読み込み時・答えが来た時に受け取る中身。
pub fn chat_state() -> serde_json::Value {
    CHAT_LAST.with(|c| {
        let c = c.borrow();
        serde_json::json!({
            "answer": c.as_ref().map(|(t, _)| t.clone()),
            "error": c.as_ref().is_some_and(|(_, e)| *e),
            "busy": crate::chat::busy(),
        })
    })
}

/// 答えが変わったら、開いているチャット欄を描き直し、高さを合わせる。
pub fn refresh_chat() {
    if !visible(CHAT) {
        return;
    }
    let Some(w) = window(CHAT) else { return };
    let rect = chat_rect();
    place_and_show(CHAT, &w, rect, false);
    let _ = w.eval(format!(
        "window.renderChat && window.renderChat({})",
        chat_state()
    ));
    set_chat_inside(rect, CHAT_PET.with(Cell::get).map_or(96, |a| a.1));
}

/// チャットの答え（チャットのスレッドから run_on_main_thread で呼ばれる）。
/// 全文はチャット欄に、吹き出しには `bubble_text` で整えた 1 行を出す（N3）。
pub fn show_chat_answer(text: &str) {
    show_chat_result(
        pet_core::chat::panel_text(text),
        pet_core::chat::bubble_text(text),
        false,
        false,
    );
}

/// チャットの失敗。理由はもう一度 `bubble_text` に通す（N3。呼び出し元を信用しない）。
/// 理由は `parse_output` が秘密を伏せた 1 行。チャット欄にはそのまま（パスは見える）、吹き出しには
/// `bubble_text` を通して出す。吹き出しで隠れたら、そう書いて「続きを見る」を付ける（理由がどこにも
/// 残らなかった。2026-10-21 検証係の指摘）。
pub fn show_chat_failure(reason: &str) {
    let panel = pet_core::chat::panel_text(reason);
    let bubble = pet_core::chat::bubble_text(reason);
    if bubble == pet_core::chat::BUBBLE_HIDDEN {
        show_chat_result(panel, FAILURE_HIDDEN.to_string(), true, true);
    } else {
        show_chat_result(panel, bubble, true, false);
    }
}

fn show_chat_result(panel: String, bubble: String, error: bool, hidden_failure: bool) {
    // 吹き出しに全部は出ていない（切り詰めた・チャット欄に回した）なら「続きを見る」
    // 失敗の理由も、140 字で切れたら全文はチャット欄（2026-10-21 検証係 2 巡目の指摘）
    let more = hidden_failure || panel != bubble;
    CHAT_LAST.with(|c| *c.borrow_mut() = Some((panel, error)));
    if visible(CHAT) {
        // 開いているチャット欄に出す（吹き出しは重ねない）
        refresh_chat();
        return;
    }
    // 承認のカードが出ている間は上書きしない（答えはチャット欄で見られる）
    if PERMS.with(|q| !q.borrow().is_empty()) {
        return;
    }
    let chars = bubble.chars().count() as u64;
    let ms = (ANSWER_BASE_MS + chars * ANSWER_PER_CHAR_MS).min(ANSWER_MAX_MS);
    let generation = STATUS_GEN.with(|g| {
        g.set(g.get() + 1);
        g.get()
    });
    render_bubble(serde_json::json!({
        "mode": "answer", "text": bubble, "more": more, "error": error,
        "lines": answer_lines(&bubble),
    }));
    let Some(handle) = app() else { return };
    let _ = std::thread::Builder::new().spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        let _ = handle.run_on_main_thread(move || {
            let current = STATUS_GEN.with(Cell::get) == generation;
            let is_answer = BUBBLE_STATE.with(|s| s.borrow()["mode"] == "answer");
            if current && is_answer {
                render_bubble(serde_json::Value::Null);
            }
        });
    });
}

/// 答えの吹き出しで折り返したときの行数の見積もり（多めに数える。半角 1 桁・全角 2 桁）。
fn answer_lines(text: &str) -> usize {
    let cols: usize = text
        .chars()
        .map(|c| if (c as u32) < 0x1100 { 1 } else { 2 })
        .sum();
    cols.div_ceil(ANSWER_COLS).max(1)
}

/// 「新しいチャット」: 会話と最後の答えを消す。
pub fn new_chat() {
    crate::chat::new_chat();
    CHAT_LAST.with(|c| *c.borrow_mut() = None);
    if BUBBLE_STATE.with(|s| s.borrow()["mode"] == "answer") {
        render_bubble(serde_json::Value::Null);
    }
    refresh_chat();
}

/// 右クリックメニューを出す。ペットのウィンドウプロシージャから呼ぶので後で実行する。
/// 右クリックしたら、カーソルが乗って出ていたバーも固定する（メニューを使っている間に消えないように）。
pub fn show_menu(at: Point, pet: Rect, dpi: u32) {
    HOVER_BAR.with(|h| h.set(false));
    open_menu(at, pet, dpi);
}

/// カーソルがペットに乗ったら、クリックと同じバー（チャット・音声入力）を出す（K 指示 2026-10-21。
/// 最初は右クリックのメニューを出していたが、K の言う「メニュー」はこのバーだった）。乗って出したバーは、
/// カーソルがペットとバーの外へ出て `HOVER_LEAVE_MS` たったら閉じる（マウスフックで見る）。
/// バー・チャット・メニューが出ている間は出さない（クリックで開いた UI に重ねない）。
pub fn show_bar_on_hover(pet: Rect, dpi: u32) {
    defer(move || {
        if popups_visible() {
            return;
        }
        HOVER_BAR.with(|h| h.set(true));
        log("bar on hover");
        open_bar(pet, dpi);
    });
}

/// ホバーで出したバーから離れたかを、少し待ってから確かめて閉じる。
/// バーの縁をかすめただけで閉じないよう、待ってからもう一度カーソルの位置を見る。
fn schedule_hover_leave_check() {
    if LEAVE_PENDING.with(Cell::get) {
        return;
    }
    LEAVE_PENDING.with(|p| p.set(true));
    let Some(handle) = app() else { return };
    let _ = std::thread::Builder::new().spawn(move || {
        std::thread::sleep(Duration::from_millis(HOVER_LEAVE_MS));
        let _ = handle.run_on_main_thread(|| {
            LEAVE_PENDING.with(|p| p.set(false));
            if !HOVER_BAR.with(Cell::get) || !visible(BAR) {
                return;
            }
            let mut p = POINT::default();
            // SAFETY: 出力先はローカル変数
            if unsafe { GetCursorPos(&mut p) }.is_err() {
                return;
            }
            let at = Point { x: p.x, y: p.y };
            if INSIDE.with(|i| i.borrow().iter().all(|r| !r.contains(at))) {
                HOVER_BAR.with(|h| h.set(false));
                log("hover bar: left -> close");
                hide(BAR);
            }
        });
    });
}

fn open_menu(at: Point, pet: Rect, dpi: u32) {
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
            if !still || popups_visible() || visible(SETTINGS) || visible(BUBBLE) {
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
                .inner_size(440.0, 600.0)
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
        // 吹き出しの「続きを見る」: チャット欄を開いて全文を出す
        "chat:more" => {
            render_bubble(serde_json::Value::Null);
            open_chat();
        }
        "chat:new" => new_chat(),
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
            crate::chat::shutdown();
            crate::pet_window::shutdown();
            crate::ipc_server::stop();
            log("quit");
            if let Some(app) = app() {
                app.exit(0);
            }
        }
        _ => {}
    });
}

// ---- 吹き出し（Phase 2）: 承認のカードと固定文言 ----

/// ペットが描くたびに呼ぶ（ウィンドウプロシージャの中なので、窓に触る処理は後回し）。
pub fn note_pet(rect: Rect, dpi: u32) {
    let moved = PET_AT.with(|c| {
        let old = c.get();
        c.set(Some((rect, dpi)));
        old != Some((rect, dpi))
    });
    if moved && visible(BUBBLE) {
        defer(place_bubble);
    }
}

fn bubble_island() -> (f64, f64) {
    BUBBLE_STATE.with(|s| {
        let s = s.borrow();
        if s["mode"] == "answer" {
            let lines = s["lines"].as_u64().unwrap_or(1).max(1);
            let more = if s["more"] == true {
                ANSWER_MORE_ROW
            } else {
                0.0
            };
            return (
                ANSWER_WIDTH,
                ANSWER_BASE + (lines - 1) as f64 * ANSWER_LINE_HEIGHT + more,
            );
        }
        if s["mode"] != "permission" {
            return STATUS_ISLAND;
        }
        let base = if s["canAllow"] == true {
            PERMISSION_ISLAND
        } else {
            PERMISSION_NOTE_ISLAND
        };
        // 要約は折り返して全文を出すので、2 行目から 1 行ごとに高くする
        let lines = s["lines"]
            .as_u64()
            .unwrap_or(1)
            .clamp(1, MAX_SUMMARY_LINES as u64);
        (base.0, base.1 + (lines - 1) as f64 * SUMMARY_LINE_HEIGHT)
    })
}

/// 要約を承認のカードで折り返したときの行数の見積もり（多めに数える）。
/// 等幅 12px の要約欄は幅 276px＝半角 38 字。字幅の誤差を見込んで 1 行 36 桁、全角は 2 桁で数える。
/// 見積もりが外れて欄からはみ出したら、ページ側が「許可」を消す（bubble.html）。
fn summary_lines(summary: &str) -> usize {
    let cols: usize = summary
        .chars()
        .map(|c| if (c as u32) < 0x1100 { 1 } else { 2 })
        .sum();
    cols.div_ceil(SUMMARY_COLS).max(1)
}

/// 吹き出しをペットの真上・中央に置く（作業領域にクランプ）。
fn place_bubble() {
    let Some((pet, dpi)) = PET_AT.with(Cell::get) else {
        return;
    };
    let Some(w) = window(BUBBLE) else { return };
    let (bw, bh) = window_size(bubble_island(), dpi);
    let bottom = pet.top + (BUBBLE_DROP * scale(dpi)).round() as i32;
    let rect = clamp_to_screen(Rect::from_xywh(
        pet.center().x - bw / 2,
        bottom - bh,
        bw,
        bh,
    ));
    place_and_show(BUBBLE, &w, rect, false);
}

fn render_bubble(state: serde_json::Value) {
    BUBBLE_STATE.with(|s| *s.borrow_mut() = state.clone());
    if state.is_null() {
        hide(BUBBLE);
        return;
    }
    let Some(w) = island(BUBBLE) else { return };
    let _ = w.eval(format!("window.render && window.render({state})"));
    place_bubble();
}

/// 吹き出しから「許可」を押せるか。見えている要約が操作の全部であるときだけ許す。
/// - hook が「要約は元の文字列と同じ」の印（`detail == "exact"`）を付けていない
///   （改行を潰した・切り詰めた・要約の無いツール）
/// - 表示順を変える／見えない書式文字（双方向制御・ゼロ幅など）を含む
/// - 折り返しても `MAX_SUMMARY_LINES` 行に収まらない
///
/// のどれかなら、ターミナルで全文を見て答えてもらう（隠れた後半や並べ替えで、見た目と違う
/// コマンドを一押しで通させないため。2026-10-21 セキュリティレビューと検証係の指摘）。
fn can_allow(ev: &WireEvent) -> bool {
    let summary = ev.summary.as_deref().unwrap_or("");
    // 文字の判定は pet-core と同じ関数（集合を 1 か所に寄せる。許可リスト）
    ev.detail.as_deref() == Some(EXACT)
        && !summary.is_empty()
        && summary.chars().all(pet_core::hook::is_plain_char)
        && summary_lines(summary) <= MAX_SUMMARY_LINES
}

/// 期限切れ・hook が問い合わせをやめた承認のカードを下げる。カードがある間は 1 秒ごとに見る
/// （hook があきらめた・ターミナルで答えて hook が終わったとき、カードが残り続けないように）。
fn prune_permissions() {
    let removed = PERMS.with(|q| {
        let mut q = q.borrow_mut();
        let before = q.len();
        q.retain(|(id, _)| crate::ipc_server::is_pending(id));
        before != q.len()
    });
    if removed {
        log("期限切れ・放置された承認のカードを下げた");
        show_front_permission();
    }
}

fn schedule_prune() {
    if PRUNING.with(Cell::get) {
        return;
    }
    let Some(handle) = app() else { return };
    PRUNING.with(|p| p.set(true));
    let spawned = std::thread::Builder::new().spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let (tx, rx) = std::sync::mpsc::channel();
            let sent = handle.run_on_main_thread(move || {
                prune_permissions();
                let empty = PERMS.with(|q| q.borrow().is_empty());
                if empty {
                    PRUNING.with(|p| p.set(false));
                }
                let _ = tx.send(empty);
            });
            // カードが無くなった・アプリが終わる（メインスレッドに届かない）なら止める
            if sent.is_err() || rx.recv().unwrap_or(true) {
                break;
            }
        }
    });
    if spawned.is_err() {
        // 作れなかったら次のカードでやり直せるようにする
        PRUNING.with(|p| p.set(false));
    }
}

/// 待っている承認の先頭を吹き出しに出す（無ければ隠す）。
fn show_front_permission() {
    let front = PERMS.with(|q| {
        let q = q.borrow();
        q.front()
            .map(|(id, ev)| (id.clone(), ev.clone(), q.len() - 1))
    });
    // 先頭のカードが入れ替わった時刻を残す（二度押しの 2 回目が次のカードに入らないように）
    let front_id = front.as_ref().map(|(id, _, _)| id.clone());
    FRONT_SINCE.with(|f| {
        let mut f = f.borrow_mut();
        if f.as_ref().map(|(id, _)| id) != front_id.as_ref() {
            *f = front_id.map(|id| (id, Instant::now()));
        }
    });
    match front {
        Some((id, ev, more)) => render_bubble(serde_json::json!({
            "mode": "permission",
            "id": id,
            "tool": pet_core::hook::sanitize_line(ev.tool.as_deref().unwrap_or(""), 60),
            "canAllow": can_allow(&ev),
            "lines": summary_lines(ev.summary.as_deref().unwrap_or("")),
            "summary": ev.summary.unwrap_or_default(),
            "more": more,
        })),
        None => render_bubble(serde_json::Value::Null),
    }
}

/// 受け口から（メインスレッドで）呼ばれる。承認のカードを出す。
pub fn show_permission(id: String, ev: WireEvent) {
    log(format!(
        "permission requested id={id} tool={}",
        ev.tool.as_deref().unwrap_or("")
    ));
    PERMS.with(|q| q.borrow_mut().push_back((id.clone(), ev)));
    show_front_permission();
    schedule_prune();
    auto_decide_for_tests(&id);
}

/// 試験用（デバッグビルドだけ）: `PET_AUTO_DECIDE=allow|deny|ask` なら 1.5 秒後にボタンと同じ関数で答える。
fn auto_decide_for_tests(id: &str) {
    if !cfg!(debug_assertions) {
        return;
    }
    let Ok(decision) = std::env::var("PET_AUTO_DECIDE") else {
        return;
    };
    // 置き場所を試験用に差し替えた時だけ効かせる（普段使いのデバッグビルドで環境変数が残っていても、
    // 承認が自動で通らないように。2026-10-21 検証係の指摘）
    if std::env::var_os("PET_CLAUDE_DATA_DIR").is_none() {
        log("PET_AUTO_DECIDE は PET_CLAUDE_DATA_DIR と一緒の時だけ効く（無視した）");
        return;
    }
    let Some(handle) = app() else { return };
    let id = id.to_string();
    let _ = std::thread::Builder::new().spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let _ = handle.run_on_main_thread(move || permission_decide(id, decision));
    });
}

/// 押されたカード `id` が、先頭になってから `FRESH_CARD_GUARD` たっていないか。
fn pressed_too_soon(front: Option<&(String, Instant)>, id: &str, now: Instant) -> bool {
    front.is_some_and(|(front, since)| front == id && now.duration_since(*since) < FRESH_CARD_GUARD)
}

/// 吹き出しのボタン（`invoke("permission_decide", {id, decision})`）。
pub fn permission_decide(id: String, decision: String) {
    // 出たばかりのカードへの「許可」「拒否」は捨てる。前のカードへの二度押しの 2 回目が、読んでいない
    // 次のカードに入るのを防ぐ（2026-10-21 検証係の指摘。ページ側でもボタンを同じ時間押せなくする）
    let fresh = FRONT_SINCE.with(|f| pressed_too_soon(f.borrow().as_ref(), &id, Instant::now()));
    if fresh && decision != "ask" {
        log(format!(
            "permission {id}: 出たばかりのカードへの {decision} を捨てた"
        ));
        return;
    }
    // ページ側でボタンを隠すだけでなく、ここでも同じ条件で止める（ページは信頼の境界にしない）
    let allowed = PERMS.with(|q| q.borrow().iter().any(|(i, ev)| *i == id && can_allow(ev)));
    let state = match decision.as_str() {
        "allow" if allowed => PermissionState::Allow,
        "allow" => {
            log(format!(
                "permission {id}: 全文が見えないので許可を受け付けない（ターミナルへ）"
            ));
            PermissionState::Ask
        }
        "deny" => PermissionState::Deny,
        _ => PermissionState::Ask,
    };
    let accepted = crate::ipc_server::decide(&id, state);
    // 押されたボタンと、実際に入れた答え（許可を受け付けなかったときは ask）を両方残す。E2E はこの行を見る
    let effective = match state {
        PermissionState::Allow => "allow",
        PermissionState::Deny => "deny",
        _ => "ask",
    };
    log(format!(
        "permission {id} pressed={decision} state={effective} (accepted={accepted})"
    ));
    PERMS.with(|q| q.borrow_mut().retain(|(i, _)| *i != id));
    show_front_permission();
}

/// そのセッションで承認の後の出来事が来た＝承認は別の経路（ターミナル）で済んだ。カードを下げる。
pub fn drop_permissions_for(session: &str) {
    let removed: Vec<String> = PERMS.with(|q| {
        let mut q = q.borrow_mut();
        let mut removed = Vec::new();
        q.retain(|(id, ev)| {
            let keep = ev.session != session && crate::ipc_server::is_pending(id);
            if !keep {
                removed.push(id.clone());
            }
            keep
        });
        removed
    });
    // 下げたカードの hook には「決めない」を返す（待ち続けさせない。2026-10-21 検証係の指摘）
    for id in &removed {
        crate::ipc_server::decide(id, PermissionState::Ask);
    }
    if !removed.is_empty() {
        show_front_permission();
    }
}

/// 短い固定文言を 3 秒出す。承認のカード・バー・メニューが出ている間は出さない。
pub fn show_status(text: &'static str) {
    let busy = PERMS.with(|q| !q.borrow().is_empty()) || popups_visible();
    if busy {
        return;
    }
    let generation = STATUS_GEN.with(|g| {
        g.set(g.get() + 1);
        g.get()
    });
    render_bubble(serde_json::json!({ "mode": "status", "text": text }));
    let Some(handle) = app() else { return };
    let _ = std::thread::Builder::new().spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(STATUS_MS));
        let _ = handle.run_on_main_thread(move || {
            let current = STATUS_GEN.with(Cell::get) == generation;
            let is_status = BUBBLE_STATE.with(|s| s.borrow()["mode"] == "status");
            if current && is_status {
                render_bubble(serde_json::Value::Null);
            }
        });
    });
}

/// bubble.html が読み込み時に聞く（作った直後の eval は読み込み前で空振りするため）。
pub fn bubble_state() -> serde_json::Value {
    BUBBLE_STATE.with(|s| s.borrow().clone())
}

// ---- 外側クリックの検出（低レベルのマウスフック。島が出ている間だけ入れる） ----

unsafe extern "system" fn mouse_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let msg = wp.0 as u32;
        if msg == WM_MOUSEMOVE && HOVER_BAR.with(Cell::get) {
            // SAFETY: WH_MOUSE_LL の lParam は MSLLHOOKSTRUCT を指す
            let info = unsafe { &*(lp.0 as *const MSLLHOOKSTRUCT) };
            let p = Point {
                x: info.pt.x,
                y: info.pt.y,
            };
            if INSIDE.with(|i| i.borrow().iter().all(|r| !r.contains(p))) {
                // フックの中では待たない。確かめて閉じるのは別スレッド経由でイベントループに回す
                schedule_hover_leave_check();
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(summary: Option<&str>, exact: bool) -> WireEvent {
        WireEvent {
            kind: "permission".into(),
            session: "s".into(),
            tool: Some("Bash".into()),
            summary: summary.map(Into::into),
            detail: exact.then(|| EXACT.into()),
        }
    }

    #[test]
    fn allow_only_when_the_whole_command_is_visible() {
        assert!(can_allow(&ev(Some("echo hi"), true)));
        // hook の印が無い（改行を潰した・切り詰めた・要約の無いツール）なら出さない
        assert!(!can_allow(&ev(Some("echo safe whoami"), false)));
        assert!(!can_allow(&ev(Some(""), true)));
        assert!(!can_allow(&ev(None, true)));
        // 見えない書式文字
        assert!(!can_allow(&ev(Some("echo \u{202E}txt.exe"), true)));
        assert!(!can_allow(&ev(Some("rm\u{200B} -rf"), true)));
        assert!(!can_allow(&ev(Some("a\u{2066}b\u{2069}"), true)));
        // 折り返して 4 行に収まるか（半角 36 桁/行・全角は 2 桁）
        assert!(can_allow(&ev(Some(&"a".repeat(120)), true)));
        assert!(can_allow(&ev(Some(&"あ".repeat(72)), true)));
        assert!(!can_allow(&ev(Some(&"あ".repeat(73)), true)));
    }

    #[test]
    fn bar_goes_under_the_pet_or_over_it_at_the_bottom_edge() {
        let work = Rect::new(0, 0, 1920, 1040);
        // 下に入る: ペットの下端から 18px 重ねて真下・中央
        let pet = Rect::from_xywh(800, 500, 200, 220);
        let r = under_or_over(&pet, (120, 72), 18, &work);
        assert_eq!((r.left, r.top), (840, 702));
        // 画面の下端にいるペット: 下に入らないので真上（上端から 18px 重ねる）
        let pet = Rect::from_xywh(800, 820, 200, 220);
        let r = under_or_over(&pet, (120, 72), 18, &work);
        assert_eq!((r.top, r.bottom), (820 + 18 - 72, 820 + 18));
        assert!(r.bottom <= pet.top + 18, "ペットとの重なりは上端の余白だけ");
        // ちょうど入るときは下
        let pet = Rect::from_xywh(800, 1040 - 72 - 220 + 18, 200, 220);
        assert_eq!(under_or_over(&pet, (120, 72), 18, &work).bottom, 1040);
    }

    #[test]
    fn presses_on_a_fresh_card_are_ignored() {
        let t0 = Instant::now();
        let front = ("b".to_string(), t0);
        // 入れ替わってすぐの押下（前のカードへの二度押しの 2 回目）は捨てる
        assert!(pressed_too_soon(
            Some(&front),
            "b",
            t0 + Duration::from_millis(100)
        ));
        // 600ms 過ぎたら受け付ける
        assert!(!pressed_too_soon(
            Some(&front),
            "b",
            t0 + Duration::from_millis(600)
        ));
        // 先頭でない id・先頭が無いときは関係ない（古い DOM の押下は permission_decide 側で別に止まる）
        assert!(!pressed_too_soon(Some(&front), "a", t0));
        assert!(!pressed_too_soon(None, "b", t0));
    }

    #[test]
    fn summary_lines_counts_wide_chars_double() {
        assert_eq!(summary_lines(""), 1);
        assert_eq!(summary_lines(&"a".repeat(36)), 1);
        assert_eq!(summary_lines(&"a".repeat(37)), 2);
        assert_eq!(summary_lines(&"あ".repeat(18)), 1);
        assert_eq!(summary_lines(&"あ".repeat(19)), 2);
    }
}
