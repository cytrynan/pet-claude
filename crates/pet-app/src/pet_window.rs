//! ペット本体の窓。Win32 の layered window にスプライトを直接描く。
//!
//! - `UpdateLayeredWindow` に画素ごとのアルファを渡すと、アルファ 0 の画素へのクリックは
//!   OS が背後の窓へ通す。だから「キャラの不透明部分だけ反応」に追加の判定は要らない
//! - `WS_EX_NOACTIVATE` と `WM_MOUSEACTIVATE → MA_NOACTIVATE` で、クリックしても
//!   作業中の窓からフォーカスを奪わない
//! - 座標はすべて物理ピクセル（Per-Monitor DPI v2）。窓の大きさはいま居る画面の DPI で決める
//!
//! 窓は 1 つだけなので、状態はメインスレッドの thread_local に置く。ウィンドウプロシージャは
//! Win32 API の中から再入される（SetCapture → WM_CAPTURECHANGED など）ので、
//! 借用中に来たメッセージは既定処理に回す。

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::time::Instant;

use image::RgbaImage;
use pet_core::behavior::{AgentEvent, Behavior, BehaviorConfig, UserEvent};
use pet_core::geometry::{Point, Rect};
use pet_core::ipc::WireEvent;
use pet_core::placement::{self, Monitor};
use pet_core::sprite::{CELL_H, CELL_W, Playback, Pose, cell_origin, rows_in};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC,
    GetMonitorInfoW, HGDIOBJ, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow, ReleaseDC,
    SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClassNameW,
    GetForegroundWindow, GetMessagePos, GetWindowRect, HWND_TOPMOST, IDC_ARROW, KillTimer,
    LoadCursorW, MA_NOACTIVATE, RegisterClassExW, SPI_SETWORKAREA, SW_SHOWNOACTIVATE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SetTimer, SetWindowPos, ShowWindow,
    ULW_ALPHA, UpdateLayeredWindow, WM_CAPTURECHANGED, WM_DISPLAYCHANGE, WM_DPICHANGED,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_RBUTTONUP, WM_SETTINGCHANGE,
    WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP,
};
use windows::core::w;

use crate::log::log;
use crate::monitors::{self, to_rect};
use crate::state::{AppState, MAX_SCALE, MIN_SCALE};
use crate::ui;

/// 行動とアニメーションの時計（約 30fps）。
const TIMER_TICK: usize = 1;
const TICK_MS: u32 = 33;
const TIMER_TOPMOST: usize = 2;
const TIMER_DISPLAY: usize = 3;
const DRAG_THRESHOLD_PX: i32 = 4;
const DEFAULT_INSET: i32 = 24;
/// この値未満のアルファは 0 に落とす（縁のかすかな影でクリックを拾わない）。
const ALPHA_CUTOFF: u8 = 24;

struct Drag {
    down_cursor: Point,
    /// 掴んだ点の、窓内での相対位置（0..1）。画面をまたいで窓の大きさが変わっても保つ。
    grab: (f32, f32),
    moved: bool,
}

struct Pet {
    hwnd: HWND,
    sheet: RgbaImage,
    rows: u32,
    state: AppState,
    /// いま描いている画面の DPI。
    dpi: u32,
    rect: Rect,
    playback: Playback,
    frame: u32,
    epoch: Instant,
    drag: Option<Drag>,
    /// (行, コマ) → BGRA 事前乗算済みの画素。大きさが変わったら捨てる。
    cache: HashMap<(u32, u32), Vec<u8>>,
    behavior: Behavior,
    pose: Pose,
    /// バー・メニューが出ていると Behavior に伝えた値
    held: bool,
    last_tick_ms: u64,
    /// 散歩で動かす物理ピクセルの端数（1px 未満を次の tick へ持ち越す）
    walk_carry: f32,
}

thread_local! {
    static PET: RefCell<Option<Pet>> = const { RefCell::new(None) };
}

fn now_ms(epoch: Instant) -> u64 {
    epoch.elapsed().as_millis() as u64
}

fn size_for(scale: f32, dpi: u32) -> (i32, i32) {
    let f = scale * dpi as f32 / 96.0;
    (
        (CELL_W as f32 * f).round() as i32,
        (CELL_H as f32 * f).round() as i32,
    )
}

/// いま処理しているメッセージが発生した時点のカーソル位置（物理ピクセル）。
/// `GetCursorPos` は「処理した時点」なので、速いドラッグでは掴んだ位置がずれる。
fn message_cursor() -> Point {
    // SAFETY: 引数なし
    let packed = unsafe { GetMessagePos() };
    Point {
        x: i32::from(packed as u16 as i16),
        y: i32::from((packed >> 16) as u16 as i16),
    }
}

/// コマを切り出し、事前乗算 → 縮小 → BGRA に並べ替える。
fn render_cell(sheet: &RgbaImage, row: u32, frame: u32, w: u32, h: u32) -> Vec<u8> {
    let (x, y) = cell_origin(row, frame);
    let mut cell = image::imageops::crop_imm(sheet, x, y, CELL_W, CELL_H).to_image();
    // 透明な黒と補間して縁が黒ずむのを防ぐため、縮小の前に事前乗算する
    for p in cell.pixels_mut() {
        let a = u16::from(p[3]);
        for c in 0..3 {
            p[c] = ((u16::from(p[c]) * a + 127) / 255) as u8;
        }
    }
    let scaled = image::imageops::resize(&cell, w, h, image::imageops::FilterType::Triangle);
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for p in scaled.pixels() {
        if p[3] < ALPHA_CUTOFF {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    out
}

impl Pet {
    fn size(&self) -> (i32, i32) {
        size_for(self.state.scale, self.dpi)
    }

    /// ポーズが変わったら最初のコマから再生し直す。描くのは呼び出し側。
    fn set_pose(&mut self, pose: Pose, now: u64) -> bool {
        if pose == self.pose {
            return false;
        }
        if std::env::var_os("PET_TRACE").is_some() || std::env::var_os("PET_FAST").is_some() {
            log(format!("pose -> {pose:?}"));
        }
        self.pose = pose;
        self.playback = Playback {
            clip: pose.clip_for(self.rows),
            started_ms: now,
        };
        self.frame = 0;
        true
    }

    /// 約 30fps で呼ばれる。行動を決め、コマを進め、散歩なら窓を動かす。
    fn tick(&mut self) {
        let now = now_ms(self.epoch);
        let dt = now.saturating_sub(self.last_tick_ms).min(200);
        self.last_tick_ms = now;

        let held = ui::popups_visible();
        if held != self.held {
            self.held = held;
            self.behavior.set_held(held, now);
        }
        let out = self.behavior.tick(now);
        let mut dirty = self.set_pose(out.pose, now);
        let (frame, _) = self.playback.frame_at(now);
        if frame != self.frame {
            self.frame = frame;
            dirty = true;
        }
        if out.vx != 0.0 && self.drag.is_none() {
            dirty |= self.walk(out.vx, dt, now);
        }
        if dirty {
            self.present();
        }
    }

    /// 散歩で横に動く。いまの画面の作業領域から出そうなら止めて向きを変えさせる。
    fn walk(&mut self, vx: f32, dt_ms: u64, now: u64) -> bool {
        let px = vx * (dt_ms as f32 / 1000.0) * (self.dpi as f32 / 96.0) + self.walk_carry;
        let step = px.trunc() as i32;
        self.walk_carry = px.fract();
        if step == 0 {
            return false;
        }
        let moved = self.rect.offset(step, 0);
        let rect = match monitors::at_point(self.rect.center()) {
            Some(m) => placement::clamp_into(&moved, &m.work),
            None => moved,
        };
        if rect.left != moved.left {
            self.walk_carry = 0.0;
            self.behavior.on_blocked(now);
        }
        let changed = rect != self.rect;
        self.rect = rect;
        changed
    }

    /// 現在のコマを現在の位置に描く。
    fn present(&mut self) {
        let (w, h) = self.size();
        let key = (self.playback.clip.row, self.frame);
        let sheet = &self.sheet;
        let pixels = self
            .cache
            .entry(key)
            .or_insert_with(|| render_cell(sheet, key.0, key.1, w as u32, h as u32));
        if let Err(e) = blit(self.hwnd, &self.rect, pixels, w, h) {
            log(format!("UpdateLayeredWindow に失敗: {e}"));
        }
        ui::note_pet(self.rect, self.dpi);
    }

    /// 窓の大きさを `dpi` に合わせる。足元（下端中央）を基準に置き直す。
    fn apply_dpi(&mut self, dpi: u32) {
        if dpi == self.dpi {
            return;
        }
        let anchor = placement::anchor_of(&self.rect);
        self.dpi = dpi;
        self.cache.clear();
        let (w, h) = self.size();
        self.rect = placement::rect_from_anchor(anchor, w, h);
        log(format!("dpi -> {dpi} size {w}x{h}"));
    }

    fn place(&mut self, rect: Rect) {
        self.rect = rect;
        self.present();
    }

    /// いまのディスプレイ構成で置き直す（起動時・画面の増減・作業領域の変化）。
    fn relayout(&mut self, reason: &str) {
        let ms = monitors::enumerate();
        let scale = self.state.scale;
        let Some((rect, how)) =
            self.state
                .positions
                .resolve(&ms, |m| size_for(scale, m.dpi), DEFAULT_INSET)
        else {
            log("ディスプレイが見つからない");
            return;
        };
        let dpi = placement::monitor_for_rect(&ms, &rect).map_or(96, |m| m.dpi);
        if dpi != self.dpi {
            self.dpi = dpi;
            self.cache.clear();
        }
        log(format!(
            "relayout({reason}) monitors={} -> {:?} at ({},{}) {}x{} dpi={dpi}",
            ms.len(),
            how,
            rect.left,
            rect.top,
            rect.width(),
            rect.height()
        ));
        self.place(rect);
    }

    fn on_button_down(&mut self) {
        let c = message_cursor();
        let (w, h) = (
            self.rect.width().max(1) as f32,
            self.rect.height().max(1) as f32,
        );
        let grab = (
            (c.x - self.rect.left) as f32 / w,
            (c.y - self.rect.top) as f32 / h,
        );
        self.drag = Some(Drag {
            down_cursor: c,
            grab,
            moved: false,
        });
        // SAFETY: 自分の窓にマウスを捕まえる
        unsafe { SetCapture(self.hwnd) };
    }

    fn on_mouse_move(&mut self) {
        let c = message_cursor();
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        if !drag.moved {
            let (dx, dy) = (c.x - drag.down_cursor.x, c.y - drag.down_cursor.y);
            if dx.abs() < DRAG_THRESHOLD_PX && dy.abs() < DRAG_THRESHOLD_PX {
                return;
            }
            drag.moved = true;
            log("drag start");
            // 借用中なので後回しにする（UI の窓に触る処理はすべて defer を通す）
            ui::defer(ui::hide_all);
            let now = now_ms(self.epoch);
            self.behavior.on_user(UserEvent::DragStart, now);
            let pose = self.behavior.tick(now).pose;
            self.set_pose(pose, now);
        }
        let grab = self.drag.as_ref().map_or((0.5, 0.5), |d| d.grab);
        // カーソルの居る画面の DPI に合わせ、その画面の作業領域に収める
        let monitor = monitors::at_point(c);
        if let Some(m) = &monitor {
            self.apply_dpi(m.dpi);
        }
        let (w, h) = self.size();
        let left = c.x - (grab.0 * w as f32).round() as i32;
        let top = c.y - (grab.1 * h as f32).round() as i32;
        let mut rect = Rect::from_xywh(left, top, w, h);
        if let Some(m) = &monitor {
            rect = placement::clamp_into(&rect, &m.work);
        }
        if std::env::var_os("PET_TRACE").is_some() {
            log(format!(
                "move cursor=({},{}) -> ({},{})",
                c.x, c.y, rect.left, rect.top
            ));
        }
        self.place(rect);
    }

    fn on_button_up(&mut self) {
        log(format!("button up drag={}", self.drag.is_some()));
        let Some(drag) = self.drag.take() else { return };
        // SAFETY: 捕まえたマウスを離す（WM_CAPTURECHANGED が再入するが既定処理に回る）
        let _ = unsafe { ReleaseCapture() };
        if drag.moved {
            self.finish_drag();
        } else {
            log(format!(
                "click at ({},{})",
                drag.down_cursor.x, drag.down_cursor.y
            ));
            self.behavior.on_user(UserEvent::Click, now_ms(self.epoch));
            ui::toggle_bar(self.rect, self.dpi);
            if std::env::var_os("PET_TRACE").is_some() {
                log("toggle_bar returned (bar is shown after this line if defer is async)");
            }
        }
    }

    fn finish_drag(&mut self) {
        let ms = monitors::enumerate();
        if let Some(m) = placement::monitor_for_rect(&ms, &self.rect).cloned() {
            self.apply_dpi(m.dpi);
            let rect = placement::clamp_into(&self.rect, &m.work);
            self.state
                .positions
                .remember(&m, placement::anchor_of(&rect));
            self.state.save();
            log(format!(
                "drag end on {} at ({},{})",
                m.key(),
                rect.left,
                rect.top
            ));
            self.place(rect);
        }
        self.behavior
            .on_user(UserEvent::DragEnd, now_ms(self.epoch));
    }

    /// 他の最前面窓に埋もれないよう取り直す。全画面の窓が手前に居る間は譲る。
    fn reassert_topmost(&self) {
        if foreground_is_fullscreen(self.hwnd) {
            return;
        }
        // SAFETY: 位置と大きさを変えず Z 順だけ変える
        let _ = unsafe {
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
            )
        };
    }
}

fn foreground_is_fullscreen(own: HWND) -> bool {
    // SAFETY: 読み取りのみの Win32 呼び出し
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() || fg == own {
            return false;
        }
        let mut class = [0u16; 32];
        let n = GetClassNameW(fg, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..n]);
        if class == "Progman" || class == "WorkerW" || class == "Shell_TrayWnd" {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(fg, &mut r).is_err() {
            return false;
        }
        let mut mi = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST), &mut mi).as_bool() {
            return false;
        }
        let (w, m) = (to_rect(r), to_rect(mi.rcMonitor));
        w.left <= m.left && w.top <= m.top && w.right >= m.right && w.bottom >= m.bottom
    }
}

/// BGRA 事前乗算済みの画素を窓に転送する（位置と大きさも同時に設定）。
fn blit(hwnd: HWND, rect: &Rect, pixels: &[u8], w: i32, h: i32) -> windows::core::Result<()> {
    // SAFETY: DC とビットマップはこの関数内で作って必ず解放する。bits は w*h*4 バイトの DIB
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let result = CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
            .and_then(|dib| {
                std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), pixels.len());
                let old = SelectObject(mem, HGDIOBJ(dib.0));
                let pos = POINT {
                    x: rect.left,
                    y: rect.top,
                };
                let size = SIZE { cx: w, cy: h };
                let src = POINT { x: 0, y: 0 };
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                let r = UpdateLayeredWindow(
                    hwnd,
                    Some(screen),
                    Some(&pos),
                    Some(&size),
                    Some(mem),
                    Some(&src),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                );
                SelectObject(mem, old);
                let _ = DeleteObject(HGDIOBJ(dib.0));
                r
            });
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        result
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    let handled = PET.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else {
            return false;
        };
        let Some(pet) = guard.as_mut() else {
            return false;
        };
        match msg {
            WM_LBUTTONDOWN => pet.on_button_down(),
            WM_MOUSEMOVE => pet.on_mouse_move(),
            WM_LBUTTONUP => pet.on_button_up(),
            WM_CAPTURECHANGED => {
                log(format!(
                    "capture changed -> 0x{:x} drag={}",
                    lp.0,
                    pet.drag.is_some()
                ));
                if pet.drag.as_ref().is_some_and(|d| d.moved) {
                    pet.drag = None;
                    pet.finish_drag();
                } else {
                    pet.drag = None;
                }
            }
            WM_RBUTTONUP => ui::show_menu(message_cursor(), pet.rect, pet.dpi),
            WM_TIMER => match wp.0 {
                TIMER_TICK => pet.tick(),
                TIMER_TOPMOST => pet.reassert_topmost(),
                // ドラッグ中は置き直さない（掴んでいるペットが保存位置へ飛ぶ）。
                // タイマーは止めずに残し、離した後の次の発火で置き直す
                TIMER_DISPLAY if pet.drag.is_some() => {}
                TIMER_DISPLAY => {
                    // SAFETY: 自分の窓のタイマーを止める
                    let _ = unsafe { KillTimer(Some(pet.hwnd), TIMER_DISPLAY) };
                    pet.relayout("display-change");
                }
                _ => return false,
            },
            WM_DISPLAYCHANGE | WM_DPICHANGED => {
                // まとめて来るので 200ms 待ってから置き直す
                // SAFETY: 自分の窓へのタイマー設定
                unsafe { SetTimer(Some(pet.hwnd), TIMER_DISPLAY, 200, None) };
            }
            WM_SETTINGCHANGE if wp.0 == SPI_SETWORKAREA.0 as usize => {
                // SAFETY: 同上
                unsafe { SetTimer(Some(pet.hwnd), TIMER_DISPLAY, 200, None) };
            }
            _ => return false,
        }
        true
    });
    if handled {
        LRESULT(0)
    } else {
        // SAFETY: 既定のウィンドウプロシージャへ委ねる
        unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
    }
}

/// ペットの窓を作って表示する。メインスレッド（Tauri のイベントループと同じスレッド）で呼ぶ。
pub fn create(sheet: RgbaImage, state: AppState) -> windows::core::Result<()> {
    // SAFETY: 窓クラスの登録と窓の生成。文字列は 'static
    let hwnd = unsafe {
        let hinstance = GetModuleHandleW(None)?.into();
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: w!("PetClaudeWindow"),
            ..Default::default()
        };
        RegisterClassExW(&class);
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            w!("PetClaudeWindow"),
            w!("pet-claude"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(hinstance),
            None,
        )?
    };
    let rows = rows_in(sheet.height());
    let walk = state.walk;
    let epoch = Instant::now();
    let mut pet = Pet {
        hwnd,
        sheet,
        rows,
        state,
        dpi: 0,
        rect: Rect::from_xywh(0, 0, 1, 1),
        playback: Playback {
            clip: Pose::Idle.clip_for(rows),
            started_ms: 0,
        },
        frame: 0,
        epoch,
        drag: None,
        cache: HashMap::new(),
        behavior: Behavior::new(behavior_config(walk), seed(), 0),
        pose: Pose::Idle,
        held: false,
        last_tick_ms: 0,
        walk_carry: 0.0,
    };
    pet.relayout("startup");
    // SAFETY: 自分の窓の表示とタイマー
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(Some(hwnd), TIMER_TICK, TICK_MS, None);
        SetTimer(Some(hwnd), TIMER_TOPMOST, 1000, None);
    }
    log(format!("pet window created rows={rows}"));
    PET.with(|cell| *cell.borrow_mut() = Some(pet));
    Ok(())
}

/// 行動の定数。`PET_FAST=1` のときは検証用に短くする（散歩 5 秒・睡眠 20 秒・Claude の状態 10 秒）。
/// `walk` が false（既定）なら散歩しない。睡眠はそのまま。
fn behavior_config(walk: bool) -> BehaviorConfig {
    let mut c = if std::env::var_os("PET_FAST").is_none() {
        BehaviorConfig::default()
    } else {
        fast_config()
    };
    if !walk {
        // 無操作がこの長さに届くことは無いので、散歩に入らない（睡眠の判定はこれより先に行われる）
        c.walk_after_ms = u64::MAX;
    }
    c
}

fn fast_config() -> BehaviorConfig {
    let d = BehaviorConfig::default();
    BehaviorConfig {
        walk_after_ms: 5_000,
        sleep_after_ms: 20_000,
        rest_min_ms: 2_000,
        rest_max_ms: 4_000,
        agent_timeout_ms: 10_000,
        ..d
    }
}

/// 散歩の向き・長さの乱数の種（起動ごとに変える）。
fn seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
}

/// 借用中（ウィンドウプロシージャの中）なら、イベントループに回してから実行する。
fn with_pet(f: impl FnOnce(&mut Pet) + Send + 'static) {
    let done = PET.with(|cell| match cell.try_borrow_mut() {
        Ok(mut guard) => {
            if let Some(pet) = guard.as_mut() {
                f(pet);
            }
            None
        }
        Err(_) => Some(f),
    });
    if let Some(f) = done {
        ui::defer(move || with_pet(f));
    }
}

/// 設定画面へ渡す値。
pub fn settings() -> serde_json::Value {
    PET.with(|cell| {
        cell.try_borrow().ok().and_then(|g| {
            g.as_ref().map(|p| {
                serde_json::json!({ "scale": p.state.scale, "tts": p.state.tts, "wake": p.state.wake, "walk": p.state.walk })
            })
        })
    })
    .unwrap_or_else(|| serde_json::json!({ "scale": crate::state::DEFAULT_SCALE, "tts": true, "wake": false, "walk": false }))
}

/// 設定画面からの変更。大きさは足元を基準に描き直す。`save` が true のときだけ state.json に書く
/// （スライダーを動かしている間は描き直すだけにして、離したときに 1 回保存する）。
pub fn set_settings(scale: f32, tts: bool, wake: bool, walk: bool, save: bool) {
    with_pet(move |pet| {
        pet.state.tts = tts;
        pet.state.wake = wake;
        if walk != pet.state.walk {
            // 定数が変わるので作り直す（散歩中なら止まり、待機から数え直す）
            pet.state.walk = walk;
            pet.behavior = Behavior::new(behavior_config(walk), seed(), now_ms(pet.epoch));
            log(format!("walk -> {walk}"));
        }
        let scale = scale.clamp(MIN_SCALE, MAX_SCALE);
        if (scale - pet.state.scale).abs() > f32::EPSILON {
            let anchor = placement::anchor_of(&pet.rect);
            pet.state.scale = scale;
            pet.cache.clear();
            let (w, h) = pet.size();
            let mut rect = placement::rect_from_anchor(anchor, w, h);
            if let Some(m) = monitors::at_point(rect.center()) {
                rect = placement::clamp_into(&rect, &m.work);
            }
            pet.place(rect);
            log(format!("scale -> {scale:.2} size {w}x{h}"));
        }
        if save {
            pet.state.save();
        }
    });
}

/// hook から届いた出来事を行動に渡す（受け口のスレッドから run_on_main_thread で呼ばれる）。
/// 対応は `pet_core::hook::to_agent_event` と同じ（WireEvent の kind と detail から戻す）。
pub fn on_wire_event(ev: &WireEvent) {
    let session = ev.session.clone();
    let agent = match ev.kind.as_str() {
        "prompt" => Some(AgentEvent::Prompt { session }),
        "tool_start" => Some(AgentEvent::ToolStart { session }),
        "permission" => Some(AgentEvent::NeedsInput { session }),
        "notification"
            if matches!(
                ev.detail.as_deref(),
                Some("permission_prompt" | "elicitation_dialog")
            ) =>
        {
            Some(AgentEvent::NeedsInput { session })
        }
        "stop" => Some(AgentEvent::Done { session }),
        "stop_failure" => Some(AgentEvent::Error { session }),
        _ => None,
    };
    // 吹き出しの固定文言（承認はカードを出すので除く）
    let phrase = match ev.kind.as_str() {
        "stop" => Some("できたよ"),
        "stop_failure" => Some("失敗しちゃった"),
        "notification" if ev.detail.as_deref() == Some("elicitation_dialog") => {
            Some("入力を待ってるよ")
        }
        _ => None,
    };
    if std::env::var_os("PET_TRACE").is_some() {
        log(format!(
            "wire event kind={} session={}",
            ev.kind, ev.session
        ));
    }
    if let Some(agent) = agent {
        with_pet(move |pet| {
            let now = now_ms(pet.epoch);
            pet.behavior.on_agent(agent, now);
        });
    }
    // 新しい依頼・応答の終わりが来た＝出ていた承認はターミナルで済んだ（並行のツール呼び出しがあるので tool_start では下げない）
    if matches!(ev.kind.as_str(), "prompt" | "stop" | "stop_failure") {
        ui::drop_permissions_for(&ev.session);
    }
    if let Some(text) = phrase {
        ui::show_status(text);
    }
}

/// 位置を既定（主画面の右下）に戻す。
pub fn reset_position() {
    with_pet(|pet| {
        pet.state.positions = Default::default();
        pet.state.save();
        pet.relayout("reset");
    });
}

/// 状態を保存して窓を閉じる。
pub fn shutdown() {
    PET.with(|cell| {
        if let Some(pet) = cell.borrow_mut().take() {
            pet.state.save();
            // SAFETY: 自分の窓を破棄する
            let _ = unsafe { DestroyWindow(pet.hwnd) };
        }
    });
}

/// ログと検証用: いまの窓の矩形とポーズ。
pub fn describe_monitors(ms: &[Monitor]) -> String {
    ms.iter()
        .map(|m| {
            format!(
                "{} work=({},{})-({},{}) dpi={} primary={}",
                m.key(),
                m.work.left,
                m.work.top,
                m.work.right,
                m.work.bottom,
                m.dpi,
                m.primary
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}
