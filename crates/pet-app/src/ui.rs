//! ペットの周りの UI（Tauri の WebView 窓）: クリック時のバー・チャット入力・右クリックメニュー・設定。
//!
//! - バーとメニューは `focusable(false)`＝作業中の窓からフォーカスを奪わない。
//!   フォーカスが無いので「外をクリックしたら閉じる」は低レベルのマウスフック（開いている間だけ）で拾う
//! - チャット入力は文字を打つので、フォーカスを取る（利用者が自分でチャットを選んだとき）。
//!   一度開いたら、ペットを左クリックするまで閉じない（`closes_on`。K 指示 2026-10-22）
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

use pet_core::ask::{Progress, Step};
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
/// チャット欄: 入力の行 48px＋モード・モデルの切替の行 32px（Claude デスクトップアプリの入力欄を写す。2026-10-27）
const CHAT_ISLAND: (f64, f64) = (360.0, 80.0);
const MENU_ISLAND: (f64, f64) = (200.0, 150.0);
/// 吹き出し: 固定文言のカプセルと、承認のカード（DESIGN.md）。
/// 固定文言は文字に合わせて広げる（`pet_core::bubble::status_island`。2026-10-27 K の報告: 長い文言が切れた）。
/// ここは窓を作るときの初期値＝最小の大きさ
const STATUS_ISLAND: (f64, f64) = (
    pet_core::bubble::STATUS_MIN_WIDTH,
    pet_core::bubble::STATUS_PAD_Y * 2.0 + pet_core::bubble::STATUS_LINE_HEIGHT,
);
/// 吹き出しのページが描画後に測った大きさの上限（論理ピクセル）。見積もりが外れた分だけ広げるための
/// 値で、ページ側の誤りで窓が画面いっぱいに広がらないよう止める
const BUBBLE_FIT_MAX: (f64, f64) = (400.0, 600.0);
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
/// チャット欄を出してから、前面を失ったら取り直す間（実機では出して 7ms で失い、入力欄に打てなかった）。
/// 人が外を押すにはこれより長くかかる（ダブルクリックの間隔 500ms 程度）ので、その操作とは取り合わない。
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
/// 質問のカード: 幅、固定部分の高さ（上下の余白 24＋見出しの行 16＋ボタン行 34＋区切り 6×3）、質問文の 1 行の
/// 高さと桁数（本文 13px・幅 292px＝半角 41 桁を多めに 38 桁で数える）、選択肢 1 つの高さ（ラベルだけ 30・
/// 説明つき 44）と間。bubble.html の .ask と同じ値。確認は scripts/verify/render-ui.js の ask-* の overflow が 0
const ASK_WIDTH: f64 = 320.0;
const ASK_BASE: f64 = 92.0;
const ASK_LINE_HEIGHT: f64 = 18.0;
const ASK_COLS: usize = 38;
const ASK_OPTION_ROW: f64 = 30.0;
const ASK_OPTION_ROW_DESC: f64 = 44.0;
const ASK_OPTION_GAP: f64 = 4.0;
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
    /// 吹き出しの中身の通し番号（`render_bubble` ごとに +1。ページが測った大きさが、今の中身のものかを見分ける）
    static BUBBLE_SEQ: Cell<u64> = const { Cell::new(0) };
    /// ページが描画後に測って「この大きさが要る」と伝えてきた島の大きさ（通し番号つき。見積もりより大きい時だけ使う）
    static BUBBLE_FIT: Cell<Option<(u64, f64, f64)>> = const { Cell::new(None) };
    /// 固定文言の世代（新しい文言が出たら古いタイマーは何もしない）
    static STATUS_GEN: Cell<u64> = const { Cell::new(0) };
    /// 承認のカードの見回り（期限切れを下げる）が動いているか
    static PRUNING: Cell<bool> = const { Cell::new(false) };
    /// 承認のカードの先頭と、それが先頭になった時刻（出たばかりのカードへの押下を捨てる）
    static FRONT_SINCE: RefCell<Option<(String, Instant)>> = const { RefCell::new(None) };
    /// 先頭の質問のカードの進み（id と、1 問ずつ集めた答え）。先頭が替わったら作り直す
    static ASK_PROGRESS: RefCell<Option<(String, Progress)>> = const { RefCell::new(None) };
    /// チャット欄を「回答」の入力に切り替えている質問の id
    static ASK_TEXT_FOR: RefCell<Option<String>> = const { RefCell::new(None) };
    /// 設定「質問にペットで答える」（state.json の ask）
    static ASK_ENABLED: Cell<bool> = const { Cell::new(true) };
    /// いま出ているバーはカーソルが乗って出したもの（離れたら閉じる）。クリック・チャット・右クリックで false
    static HOVER_BAR: Cell<bool> = const { Cell::new(false) };
    /// ホバーのバーから離れたかの確認を待っている
    static LEAVE_PENDING: Cell<bool> = const { Cell::new(false) };
    /// チャットの最後の答え（チャット欄に出す全文, 失敗か）。「新しいチャット」で消す
    static CHAT_LAST: RefCell<Option<(String, bool)>> = const { RefCell::new(None) };
    /// 音声入力で聞き取って送った文（チャット欄に K の発言として出す。打って送った時と「新しいチャット」で消す）
    static VOICE_HEARD: RefCell<Option<String>> = const { RefCell::new(None) };
    /// チャット欄を開いた時刻。開いた直後の前面の喪失を 1 回だけ取り直すのに使う（使ったら消す）
    static CHAT_OPENED_AT: Cell<Option<Instant>> = const { Cell::new(None) };
    /// チャット欄のメニュー（モード・モデル）が開いている間の高さ（論理ピクセル。0 は閉じている）。
    /// メニューは入力欄の上に伸びるので、答えの欄より高ければその分だけ窓を広げる
    static CHAT_MENU_HEIGHT: Cell<f64> = const { Cell::new(0.0) };
    /// ペットが動いたあとのチャット欄の置き直しを予約済みか（ドラッグ中の描画ごとにスレッドを作らない）
    static CHAT_PLACE_PENDING: Cell<bool> = const { Cell::new(false) };
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
        // フォーカスが外れても閉じない（ペットの左クリックでだけ閉じる＝`closes_on`）。ただし出した直後
        // （CHAT_FOCUS_GRACE 以内）の 1 回は前面を取り直す。窓を作った直後の 1 回目は 7ms で前面を失い、
        // 入力欄に打てなかった（2026-10-21 K の実機確認）。取り直しは 1 回だけ（外を押した時と取り合わない）
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
                    }
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

/// 島を閉じるきっかけ。どれで何が閉じるかは `closes_on` だけで決める。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Close {
    /// ペットの左クリック（ドラッグせずに離した）
    PetClick,
    /// 島とペットの外を押した（マウスフック）
    Outside,
    /// ペットをドラッグし始めた
    Drag,
    /// Esc・メニューの「バーを隠す」「位置をリセット」
    Dismiss,
}

/// `label` の島が `why` で閉じるか。チャット欄はペットの左クリックでだけ閉じる（K 指示 2026-10-22
/// 「一度開いたら pet を左クリックするまで閉じないようにして」）。外を押す・ドラッグ・Esc では閉じない
/// （Esc は日本語入力の変換の取り消しにも使うので、閉じると打っている途中の文が消える）。
/// バーとメニューは今までどおり、どのきっかけでも閉じる。
fn closes_on(label: &str, why: Close) -> bool {
    label != CHAT || why == Close::PetClick
}

/// `why` で閉じる島を全部隠す。
pub fn close(why: Close) {
    log(format!("popups hidden ({why:?})"));
    for l in POPUPS {
        if !closes_on(l, why) {
            continue;
        }
        if let Some(w) = window(l) {
            let _ = w.hide();
        }
        mark_shown(l, false);
    }
    after_hide();
}

/// ペットのクリックでバーを出す／もう一度で隠す。ペットのウィンドウプロシージャから呼ぶので後で実行する。
/// チャット欄が出ていれば閉じる（チャット欄を閉じるのはこのクリックだけ）。
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
            close(Close::PetClick);
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
    // 打っている間に AivisSpeech を起こしておく（答えを話すまでの待ちを隠す）
    crate::tts::prewarm();
    if PET_AT.with(Cell::get).is_none() {
        return;
    }
    let Some(chat) = island(CHAT) else { return };
    // 前に閉じた時にメニューが開いたままなら、その分の高さを捨てて閉じた形で出す（窓が作り直されて
    // ページは閉じた形なのに、窓だけ高いままになる）
    CHAT_MENU_HEIGHT.with(|h| h.set(0.0));
    let rect = chat_rect();
    if let Some(bar) = window(BAR) {
        let _ = bar.hide();
    }
    mark_shown(BAR, false);
    CHAT_OPENED_AT.with(|t| t.set(Some(Instant::now())));
    place_and_show(CHAT, &chat, rect, true);
    let _ = chat.eval("window.closeChatMenu && window.closeChatMenu()");
    let _ = chat.eval(format!(
        "window.renderChat && window.renderChat({})",
        chat_state()
    ));
    let _ = chat.eval("window.focusInput && window.focusInput()");
    // バーを隠したので、外を押した時の見張り（マウスフック）も外す。チャット欄は外を押しても閉じない
    after_hide();
    log("chat shown");
}

/// チャット欄の島の大きさ。答えがあれば入力欄の上に答えの欄を足す。
fn chat_island() -> (f64, f64) {
    // 待ち中も答えの欄を出す（「新しいチャット」で取り消せるように。2026-10-21 検証係の指摘）。
    // 質問への「回答」の入力中は、答えの欄に質問文を出す
    let has_answer = CHAT_LAST.with(|c| c.borrow().is_some())
        || crate::chat::busy()
        || ask_text_target().is_some();
    let answer = if has_answer {
        CHAT_GAP + CHAT_ANSWER_HEIGHT
    } else {
        0.0
    };
    // 開いているメニューは答えの欄に重ねて出す。答えの欄より高い分だけ（答えが無ければ全部）上に足す
    let menu = CHAT_MENU_HEIGHT.with(Cell::get);
    let menu = if menu > 0.0 { CHAT_GAP + menu } else { 0.0 };
    (CHAT_ISLAND.0, CHAT_ISLAND.1 + answer.max(menu))
}

/// チャット欄のメニューを開いた（高さ）・閉じた（0）。窓をその分だけ広げて置き直す（中身は描き直さない）。
/// 上限は `BUBBLE_FIT_MAX` の高さ（ページ側の誤りで窓が画面いっぱいに広がらないように）。
pub fn set_chat_menu_height(height: f64) {
    let height = if height.is_finite() {
        height.clamp(0.0, BUBBLE_FIT_MAX.1)
    } else {
        0.0
    };
    // WebView2 のコールバック中なので、窓の操作は後で
    defer(move || {
        CHAT_MENU_HEIGHT.with(|h| h.set(height));
        place_chat();
    });
}

/// チャット欄の窓の矩形。バーと同じく、ペットの真下に置き、作業領域の下に収まらなければ真上に置く。
/// 答えの欄が足されて高くなったときも同じ規則で置き直す（入力欄の位置を保つ置き方だと、画面の下の方では
/// 答えの欄がペットに重なり、ペットに隠れた。2026-10-22 K の指示「端っこでチャットを開くと重なる問題も直して」）。
/// ペットの今の位置を使う（チャット欄は開いたままペットをドラッグできるので、ペットについて行く）。
fn chat_rect() -> Rect {
    let Some((pet, dpi)) = PET_AT.with(Cell::get) else {
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
            "heard": VOICE_HEARD.with(|h| h.borrow().clone()),
            // 質問への「回答」の入力中（id と質問文）。chat.html は送信を ask_action に向ける
            "ask": ask_text_target(),
            // いま効いているモデル・エフォート・権限モード（メニューの値か設定の既定）。下の行の表示に使う
            "options": crate::chat::options(crate::pet_window::chat_setup().defaults).to_json(),
        })
    })
}

/// 答えが変わったら、開いているチャット欄を描き直し、高さを合わせる。
pub fn refresh_chat() {
    if !visible(CHAT) {
        return;
    }
    let Some(w) = window(CHAT) else { return };
    place_and_show(CHAT, &w, chat_rect(), false);
    let _ = w.eval(format!(
        "window.renderChat && window.renderChat({})",
        chat_state()
    ));
}

/// ペットが動いたら（ドラッグ・位置のリセット・画面の変更）、開いているチャット欄をペットの真下へ置き直す。
/// 中身は描き直さず（答えの欄のスクロールを保つ）、出し直しもしない（`show()` で前面を取らない）。
fn place_chat() {
    CHAT_PLACE_PENDING.with(|p| p.set(false));
    if !visible(CHAT) {
        return;
    }
    if let Some(w) = window(CHAT) {
        let rect = chat_rect();
        let _ = w.set_position(PhysicalPosition::new(rect.left, rect.top));
        let _ = w.set_size(PhysicalSize::new(rect.width() as u32, rect.height() as u32));
    }
}

/// チャットの答え（チャットのスレッドから run_on_main_thread で呼ばれる）。
/// 全文はチャット欄に、吹き出しには `bubble_text` で整えた 1 行を出す（N3）。
pub fn show_chat_answer(text: &str) {
    crate::tts::answer(text);
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
    crate::tts::phrase(pet_core::voice::PHRASE_CHAT_FAILED);
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
    VOICE_HEARD.with(|h| *h.borrow_mut() = None);
    if BUBBLE_STATE.with(|s| s.borrow()["mode"] == "answer") {
        render_bubble(serde_json::Value::Null);
    }
    refresh_chat();
}

/// 音声入力で聞き取った文を、チャット欄に K の発言として出す（送る直前に呼ぶ）。
pub fn show_voice_text(text: &str) {
    VOICE_HEARD.with(|h| *h.borrow_mut() = Some(pet_core::chat::panel_text(text)));
    refresh_chat();
}

/// 打って送った: 前に聞き取った文は消す（別の発言の上に出さない）。
pub fn clear_voice_text() {
    VOICE_HEARD.with(|h| *h.borrow_mut() = None);
}

/// 録音中かをバーのボタンに出す。録り終えたら（固定していた）バーを閉じる。
pub fn refresh_bar() {
    let recording = crate::voice::recording();
    if let Some(w) = window(BAR) {
        let _ = w.eval(format!(
            "window.renderBar && window.renderBar({})",
            serde_json::json!({ "recording": recording })
        ));
    }
    if !recording && visible(BAR) && !HOVER_BAR.with(Cell::get) {
        hide(BAR);
    }
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

/// 隠した後の片付け。外を押した時の見張り（マウスフック）は、バーかメニューが出ている間だけ要る
/// （チャット欄は外を押しても閉じないので、チャット欄だけの間は全マウス操作を見張らない）。
fn after_hide() {
    if !visible(BAR) && !visible(MENU) {
        uninstall_hook();
    }
    if !popups_visible() && !visible(SETTINGS) {
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
        // 吹き出しのページが描画後に測った大きさ（見積もりが外れて中身がはみ出した時だけ来る）
        a if a.starts_with("bubble:fit:") => bubble_fit(a),
        // 音声入力（S11）: 録音中はバーを固定して出したままにする（もう一度押すと止める。V5）
        "voice" => {
            HOVER_BAR.with(|h| h.set(false));
            if let Some(app) = app() {
                crate::voice::toggle(app);
            }
        }
        "close" => close(Close::Dismiss),
        "menu:settings" => {
            hide(MENU);
            open_settings();
        }
        "menu:hide-bar" => close(Close::Dismiss),
        "menu:reset" => {
            close(Close::Dismiss);
            crate::pet_window::reset_position();
        }
        "menu:quit" => {
            crate::chat::shutdown();
            crate::voice::shutdown();
            crate::tts::shutdown();
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
    if moved && visible(CHAT) && !CHAT_PLACE_PENDING.with(|p| p.replace(true)) {
        defer(place_chat);
    }
}

/// 吹き出しの島の大きさ。文字から見積もった大きさと、ページが測って伝えてきた大きさの大きい方
/// （固定文言と答えだけ。承認のカードは行数の上限で「許可」を出さない決まりがあるので広げない）
fn bubble_island() -> (f64, f64) {
    let (w, h) = estimated_bubble_island();
    let fits = BUBBLE_STATE.with(|s| {
        let s = s.borrow();
        s["mode"] == "status" || s["mode"] == "answer"
    });
    match (fits, BUBBLE_FIT.with(Cell::get)) {
        (true, Some((seq, fw, fh))) if seq == BUBBLE_SEQ.with(Cell::get) => (w.max(fw), h.max(fh)),
        _ => (w, h),
    }
}

/// 中身（文字数・行数）から見積もった島の大きさ
fn estimated_bubble_island() -> (f64, f64) {
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
        if s["mode"] == "ask" {
            return (ASK_WIDTH, s["height"].as_f64().unwrap_or(ASK_BASE));
        }
        if s["mode"] == "status" {
            return pet_core::bubble::status_island(s["text"].as_str().unwrap_or(""));
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

fn render_bubble(mut state: serde_json::Value) {
    // 中身ごとの通し番号（ページが測った大きさを、古い中身のものと取り違えないため）
    let seq = BUBBLE_SEQ.with(|q| {
        q.set(q.get() + 1);
        q.get()
    });
    if let Some(o) = state.as_object_mut() {
        o.insert("seq".into(), seq.into());
    }
    BUBBLE_FIT.with(|f| f.set(None));
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
        Some((id, ev, more)) if ev.kind == "ask_question" => render_ask(id, ev, more),
        Some((id, ev, more)) => render_bubble(serde_json::json!({
            "mode": "permission",
            "id": id,
            "tool": pet_core::hook::sanitize_line(ev.tool.as_deref().unwrap_or(""), 60),
            "canAllow": can_allow(&ev),
            // 「決めない」ボタンと注記の文言は来た先で変える（アプリから来たのに「ターミナルで」と出さない）
            "askLabel": origin_of(&ev).ask_label(),
            "note": origin_of(&ev).note(),
            "lines": summary_lines(ev.summary.as_deref().unwrap_or("")),
            "summary": ev.summary.unwrap_or_default(),
            "more": more,
        })),
        None => render_bubble(serde_json::Value::Null),
    }
}

/// 承認の要求の来た先。古い pet-hook（`origin` を付けない）からの要求は「分からない」。
fn origin_of(ev: &WireEvent) -> pet_core::origin::Origin {
    pet_core::origin::Origin::from_wire(ev.origin.as_deref())
}

/// 受け口から（メインスレッドで）呼ばれる。承認のカードを出す（質問は `show_question`）。
pub fn show_permission(id: String, ev: WireEvent) {
    if ev.kind == "ask_question" {
        show_question(id, ev);
        return;
    }
    log(format!(
        "permission requested id={id} tool={}",
        ev.tool.as_deref().unwrap_or("")
    ));
    PERMS.with(|q| q.borrow_mut().push_back((id.clone(), ev)));
    crate::tts::phrase(pet_core::voice::PHRASE_PERMISSION);
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

// ---- 吹き出し: 質問のカード（AskUserQuestion。.steering/20261027-ask-question/brief.md） ----

/// 設定「質問にペットで答える」。オフなら質問のカードを出さず、知らせるだけにする。
pub fn set_ask_enabled(on: bool) {
    ASK_ENABLED.with(|a| a.set(on));
}

/// 受け口から（メインスレッドで）呼ばれる。質問のカードを出す。設定がオフ・吹き出しに出せない質問
/// （URL/パス/コード/秘密らしい文字列・形が違う）なら、知らせるだけで hook には「決めない」を返す
/// （Claude Code のいつもの画面が出る）。
fn show_question(id: String, ev: WireEvent) {
    let presentable = ev
        .questions
        .as_deref()
        .is_some_and(pet_core::ask::presentable);
    let enabled = ASK_ENABLED.with(Cell::get);
    log(format!(
        "ask requested id={id} questions={} enabled={enabled} presentable={presentable}",
        ev.questions.as_ref().map_or(0, Vec::len)
    ));
    if !enabled || !presentable {
        crate::ipc_server::decide(&id, PermissionState::Ask);
        show_status(pet_core::voice::PHRASE_QUESTION);
        crate::tts::phrase(pet_core::voice::PHRASE_QUESTION);
        return;
    }
    PERMS.with(|q| q.borrow_mut().push_back((id.clone(), ev)));
    crate::tts::phrase(pet_core::voice::PHRASE_QUESTION);
    show_front_permission();
    schedule_prune();
    auto_ask_for_tests(&id);
}

/// 質問文をカードで折り返したときの行数の見積もり（多めに数える。本文 13px・幅 292px＝半角 41 桁を 38 桁で）。
fn ask_lines(question: &str) -> usize {
    let cols: usize = question
        .chars()
        .map(|c| if (c as u32) < 0x1100 { 1 } else { 2 })
        .sum();
    cols.div_ceil(ASK_COLS).max(1)
}

/// 選択肢の欄の高さ（ラベルだけ 30・説明つき 44・最後の「回答を書く…」30・間 4）。bubble.html と同じ値。
fn ask_options_height(options: &[pet_core::ask::Choice]) -> f64 {
    let rows: f64 = options
        .iter()
        .map(|c| {
            if c.display_description().is_empty() {
                ASK_OPTION_ROW
            } else {
                ASK_OPTION_ROW_DESC
            }
        })
        .sum();
    rows + ASK_OPTION_ROW + ASK_OPTION_GAP * options.len() as f64
}

/// 先頭の質問のカードの進みに触る。`id` が先頭でなければ None。
fn with_progress<R>(id: &str, f: impl FnOnce(&mut Progress) -> R) -> Option<R> {
    ASK_PROGRESS.with(|p| {
        let mut p = p.borrow_mut();
        let (i, prog) = p.as_mut()?;
        (i == id).then(|| f(prog))
    })
}

/// 質問のカードを描く。進み（`Progress`）は先頭のカードの id ごとに 1 つ（先頭が替わったら作り直す）。
fn render_ask(id: String, ev: WireEvent, more: usize) {
    let is_same = ASK_PROGRESS.with(|p| p.borrow().as_ref().is_some_and(|(i, _)| *i == id));
    if !is_same {
        let questions = ev.questions.clone().unwrap_or_default();
        ASK_PROGRESS.with(|p| *p.borrow_mut() = Some((id.clone(), Progress::new(questions))));
    }
    let ask_label = origin_of(&ev).ask_label();
    let state = with_progress(&id, |prog| {
        let q = prog.current()?;
        let counter = if prog.total() > 1 {
            format!(" · {}/{}", prog.cursor() + 1, prog.total())
        } else {
            String::new()
        };
        let header = match q.display_header() {
            h if h.is_empty() => "質問".to_string(),
            h => h,
        };
        let options: Vec<_> = q
            .options
            .iter()
            .map(|c| {
                serde_json::json!({
                    "label": c.display_label(),
                    "description": c.display_description(),
                    "picked": prog.picked().contains(&c.label),
                })
            })
            .collect();
        let question = q.display_question();
        let lines = ask_lines(&question);
        Some(serde_json::json!({
            "mode": "ask",
            "id": id,
            // 先頭のカードと質問の番号。変わったら出たばかり扱い（600ms 押せない）にする
            "key": format!("{id}:{}", prog.cursor()),
            "header": format!("{header}{counter}"),
            "question": question,
            "options": options,
            "multi": q.multi_select,
            "more": more,
            "askLabel": ask_label,
            "height": ASK_BASE + lines as f64 * ASK_LINE_HEIGHT + ask_options_height(&q.options),
        }))
    })
    .flatten();
    match state {
        Some(s) => render_bubble(s),
        // 質問が無い・全部答えた後に描くことは無いはず。来たら任せる
        None => finish_ask(&id, PermissionState::Ask, None),
    }
}

/// 質問のカードを終える（答え・拒否・任せる）。表から消し、次のカードを出す。「回答」で開いた
/// チャット欄は閉じる。
fn finish_ask(id: &str, state: PermissionState, replies: Option<Vec<pet_core::ask::Reply>>) {
    let accepted = match replies {
        Some(r) => crate::ipc_server::decide_with_answers(id, r),
        None => crate::ipc_server::decide(id, state),
    };
    let effective = match state {
        PermissionState::Allow => "allow",
        PermissionState::Deny => "deny",
        _ => "ask",
    };
    // E2E はこの行を見る
    log(format!("ask {id} state={effective} (accepted={accepted})"));
    PERMS.with(|q| q.borrow_mut().retain(|(i, _)| *i != id));
    ASK_PROGRESS.with(|p| {
        let mut p = p.borrow_mut();
        if p.as_ref().is_some_and(|(i, _)| i == id) {
            *p = None;
        }
    });
    let was_text = ASK_TEXT_FOR.with(|t| {
        let mut t = t.borrow_mut();
        let mine = t.as_deref() == Some(id);
        if mine {
            *t = None;
        }
        mine
    });
    if was_text && visible(CHAT) {
        hide(CHAT);
    }
    show_front_permission();
}

/// 1 問答えた後。全部そろえば hook に返し、残りがあれば次の質問を描く（チャット欄が「回答」の入力なら
/// 次の質問に合わせて描き直す）。
fn after_ask_step(id: &str, step: Step) {
    match step {
        Step::Done(replies) => finish_ask(id, PermissionState::Allow, Some(replies)),
        Step::Next => {
            // 次の質問は出たばかり扱い（同じ場所の二度押しが次の質問に入らないように）
            FRONT_SINCE.with(|f| *f.borrow_mut() = Some((id.to_string(), Instant::now())));
            show_front_permission();
            refresh_chat();
        }
        Step::Stay => show_front_permission(),
        Step::Rejected => log(format!("ask {id}: 受け付けない答え")),
    }
}

/// 質問のカードのボタン（`invoke("ask_action", {id, action, value})`）と、チャット欄の「回答」の送信。
///
/// - `pick`（value＝選択肢の番号）: 1 つ選ぶ質問ならその答え、複数選択なら付け外し
/// - `submit`: 複数選択の「決定」
/// - `text`: チャット欄を「回答」の入力に切り替えて開く
/// - `answer`（value＝自由文）: チャット欄から。戻り値は入力欄に出す一言（受け付けたら "ok"）
/// - `deny` / `ask`: 拒否・Claude Code の画面に任せる
///
/// 出たばかりのカード（600ms 以内）への pick・submit・deny は捨てる（承認のカードと同じ）。
pub fn ask_action(id: String, action: String, value: String) -> String {
    let is_front = ASK_PROGRESS.with(|p| p.borrow().as_ref().is_some_and(|(i, _)| *i == id));
    if !is_front {
        log(format!("ask {id}: もう無いカードへの {action}"));
        return "この質問はもう終わっています".to_string();
    }
    let fresh = FRONT_SINCE.with(|f| pressed_too_soon(f.borrow().as_ref(), &id, Instant::now()));
    if fresh && matches!(action.as_str(), "pick" | "submit" | "deny") {
        log(format!(
            "ask {id}: 出たばかりのカードへの {action} を捨てた"
        ));
        return String::new();
    }
    log(format!("ask {id} action={action}"));
    match action.as_str() {
        "text" => {
            ASK_TEXT_FOR.with(|t| *t.borrow_mut() = Some(id));
            defer(open_chat);
            "ok".to_string()
        }
        "answer" => match with_progress(&id, |p| p.answer_text(&value)) {
            Some(Step::Rejected) | None => {
                format!("答えは 1〜{} 字で", pet_core::ask::FREE_TEXT_MAX_CHARS)
            }
            Some(step) => {
                after_ask_step(&id, step);
                "ok".to_string()
            }
        },
        "pick" => {
            let label = value.parse::<usize>().ok().and_then(|i| {
                with_progress(&id, |p| {
                    p.current()
                        .and_then(|q| q.options.get(i))
                        .map(|c| c.label.clone())
                })
                .flatten()
            });
            match label {
                Some(l) => {
                    if let Some(step) = with_progress(&id, |p| p.pick(&l)) {
                        after_ask_step(&id, step);
                    }
                }
                None => log(format!("ask {id}: 無い選択肢 {value}")),
            }
            "ok".to_string()
        }
        "submit" => {
            if let Some(step) = with_progress(&id, Progress::submit) {
                after_ask_step(&id, step);
            }
            "ok".to_string()
        }
        "deny" => {
            finish_ask(&id, PermissionState::Deny, None);
            "ok".to_string()
        }
        _ => {
            finish_ask(&id, PermissionState::Ask, None);
            "ok".to_string()
        }
    }
}

/// チャット欄が「回答」の入力になっている質問（id と、いま答える質問文）。無ければ None。
fn ask_text_target() -> Option<serde_json::Value> {
    let id = ASK_TEXT_FOR.with(|t| t.borrow().clone())?;
    with_progress(&id, |p| {
        p.current()
            .map(|q| serde_json::json!({ "id": id, "question": q.display_question() }))
    })
    .flatten()
}

/// 試験用（デバッグビルドだけ）: `PET_AUTO_ASK=pick:last|pick:first|free:<文>|deny|ask` なら 1.5 秒後に
/// ボタンと同じ関数で答える（複数の質問・複数選択は 0.7 秒ごとに 1 手ずつ）。`PET_CLAUDE_DATA_DIR` と
/// 一緒の時だけ効く（承認の `PET_AUTO_DECIDE` と同じ理由）。
fn auto_ask_for_tests(id: &str) {
    if !cfg!(debug_assertions) {
        return;
    }
    let Ok(spec) = std::env::var("PET_AUTO_ASK") else {
        return;
    };
    if std::env::var_os("PET_CLAUDE_DATA_DIR").is_none() {
        log("PET_AUTO_ASK は PET_CLAUDE_DATA_DIR と一緒の時だけ効く（無視した）");
        return;
    }
    schedule_auto_ask(id.to_string(), spec, Duration::from_millis(1500), 12);
}

fn schedule_auto_ask(id: String, spec: String, delay: Duration, left: u32) {
    let Some(handle) = app() else { return };
    let _ = std::thread::Builder::new().spawn(move || {
        std::thread::sleep(delay);
        let _ = handle.run_on_main_thread(move || {
            let (action, value) = match spec.split_once(':') {
                Some(("pick", which)) => {
                    let Some((n, multi, picked)) = with_progress(&id, |p| {
                        p.current()
                            .map(|q| (q.options.len(), q.multi_select, p.picked().len()))
                    })
                    .flatten() else {
                        return;
                    };
                    if multi && picked > 0 {
                        ("submit".to_string(), String::new())
                    } else {
                        let i = if which == "first" {
                            0
                        } else {
                            n.saturating_sub(1)
                        };
                        ("pick".to_string(), i.to_string())
                    }
                }
                Some(("free", text)) => ("answer".to_string(), text.to_string()),
                _ => (spec.clone(), String::new()),
            };
            let _ = ask_action(id.clone(), action, value);
            let more = with_progress(&id, |p| p.current().is_some()).unwrap_or(false);
            if more && left > 0 {
                schedule_auto_ask(id, spec, Duration::from_millis(700), left - 1);
            }
        });
    });
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
    // 行数はページが見た目（1 行＝カプセル・複数行＝角丸の箱）を選ぶのに使う
    let lines = pet_core::bubble::wrap(
        text,
        pet_core::bubble::STATUS_MAX_WIDTH - pet_core::bubble::STATUS_PAD_X * 2.0,
    )
    .lines;
    render_bubble(serde_json::json!({ "mode": "status", "text": text, "lines": lines }));
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

/// `bubble:fit:<seq>:<w>:<h>`（ページが描画後に測った、中身が収まる島の大きさ。論理ピクセル）。
/// 今の中身のものなら覚えて、見積もりより大きければ窓を広げる。縮めはしない（描いた後に縮むと見た目が跳ぶ）
fn bubble_fit(action: &str) {
    let Some((seq, w, h)) = parse_fit(action) else {
        log(format!("bubble:fit を読めない: {action}"));
        return;
    };
    if seq != BUBBLE_SEQ.with(Cell::get) {
        return;
    }
    let before = bubble_island();
    BUBBLE_FIT.with(|f| f.set(Some((seq, w, h))));
    if bubble_island() != before && visible(BUBBLE) {
        place_bubble();
    }
}

/// `bubble:fit:<seq>:<w>:<h>` を読む。大きさは `BUBBLE_FIT_MAX` で止める（ページの誤りで画面いっぱいに
/// 広がらないように）。正の数でなければ `None`
fn parse_fit(action: &str) -> Option<(u64, f64, f64)> {
    let mut it = action.strip_prefix("bubble:fit:")?.split(':');
    let seq: u64 = it.next()?.parse().ok()?;
    let w: f64 = it.next()?.parse().ok()?;
    let h: f64 = it.next()?.parse().ok()?;
    if it.next().is_some() || !(w > 0.0 && h > 0.0) || !w.is_finite() || !h.is_finite() {
        return None;
    }
    Some((seq, w.min(BUBBLE_FIT_MAX.0), h.min(BUBBLE_FIT_MAX.1)))
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
                defer(|| close(Close::Outside));
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
            origin: None,
            questions: None,
        }
    }

    #[test]
    fn card_wording_follows_origin() {
        use pet_core::origin::Origin;
        let mut e = ev(Some("ls"), true);
        assert_eq!(origin_of(&e), Origin::Unknown);
        e.origin = Some(Origin::DesktopApp.as_str().into());
        assert_eq!(origin_of(&e).ask_label(), "Claude アプリで");
        e.origin = Some(Origin::Terminal.as_str().into());
        assert_eq!(origin_of(&e).ask_label(), "ターミナルで");
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
    fn chat_closes_only_on_a_pet_click() {
        // チャット欄: ペットの左クリックでだけ閉じる（K 指示 2026-10-22）
        assert!(closes_on(CHAT, Close::PetClick));
        for why in [Close::Outside, Close::Drag, Close::Dismiss] {
            assert!(!closes_on(CHAT, why), "{why:?}");
        }
        // バーとメニューは今までどおり、どのきっかけでも閉じる
        for label in [BAR, MENU] {
            for why in [Close::PetClick, Close::Outside, Close::Drag, Close::Dismiss] {
                assert!(closes_on(label, why), "{label} {why:?}");
            }
        }
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
    fn parse_fit_reads_seq_and_size_and_caps_it() {
        assert_eq!(parse_fit("bubble:fit:3:320:60"), Some((3, 320.0, 60.0)));
        assert_eq!(
            parse_fit("bubble:fit:3:320.5:60.25"),
            Some((3, 320.5, 60.25))
        );
        // 上限で止める
        assert_eq!(
            parse_fit("bubble:fit:1:9999:9999"),
            Some((1, BUBBLE_FIT_MAX.0, BUBBLE_FIT_MAX.1))
        );
        // 壊れた形・0 以下・余分な項・NaN は捨てる
        assert_eq!(parse_fit("bubble:fit:1:320"), None);
        assert_eq!(parse_fit("bubble:fit:1:0:60"), None);
        assert_eq!(parse_fit("bubble:fit:1:-1:60"), None);
        assert_eq!(parse_fit("bubble:fit:1:320:60:7"), None);
        assert_eq!(parse_fit("bubble:fit:x:320:60"), None);
        assert_eq!(parse_fit("bubble:fit:1:NaN:60"), None);
        assert_eq!(parse_fit("chat:more"), None);
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
