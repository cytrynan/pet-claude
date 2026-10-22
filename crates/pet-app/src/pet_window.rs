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

use pet_core::behavior::{AgentEvent, Behavior, BehaviorConfig, UserEvent};
use pet_core::chat::AuthMode;
use pet_core::clips::{ClipKind, Player, Show, Still};
use pet_core::geometry::{Point, Rect};
use pet_core::ipc::WireEvent;
use pet_core::pixels::{fit, render_bgra, stretch_up};
use pet_core::placement::{self, Monitor};
use pet_core::sprite::{CELL_H, CELL_W, Playback, Pose, SheetLayout, breath_level, breath_scale};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetMonitorInfoW,
    HBITMAP, HDC, HGDIOBJ, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClassNameW,
    GetCursorPos, GetForegroundWindow, GetMessagePos, GetWindowRect, HWND_TOPMOST, IDC_ARROW,
    KillTimer, LoadCursorW, MA_NOACTIVATE, RegisterClassExW, SPI_SETWORKAREA, SW_SHOWNOACTIVATE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SetTimer, SetWindowPos, ShowWindow,
    ULW_ALPHA, UpdateLayeredWindow, WM_CAPTURECHANGED, WM_DISPLAYCHANGE, WM_DPICHANGED,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_RBUTTONUP, WM_SETTINGCHANGE,
    WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP, WindowFromPoint,
};
use windows::core::w;

use crate::clipstore::{ClipCache, ClipFrames, ClipPack};
use crate::log::log;
use crate::monitors::{self, to_rect};
use crate::pets::{self, CellStore, LoadedSheet, PetEntry};
use crate::state::{AppState, MAX_SCALE, MIN_SCALE};
use crate::ui;

/// 行動とアニメーションの時計（約 30fps）。
const TIMER_TICK: usize = 1;
const TICK_MS: u32 = 33;
const TIMER_TOPMOST: usize = 2;
const TIMER_DISPLAY: usize = 3;
/// カーソルがペットに乗ってからバーを出すまでの待ち（K 指示 2026-10-21「乗った時点で出す」）。
/// 0 にすると、ペットの上を通り過ぎただけでバーが出入りしてちらつくので、通過と区別できる最小に近い値にした
const TIMER_HOVER: usize = 4;
const HOVER_DWELL_MS: u32 = 200;
const DRAG_THRESHOLD_PX: i32 = 4;
const DEFAULT_INSET: i32 = 24;

struct Drag {
    down_cursor: Point,
    /// 掴んだ点の、窓内での相対位置（0..1）。画面をまたいで窓の大きさが変わっても保つ。
    grab: (f32, f32),
    moved: bool,
}

struct Pet {
    hwnd: HWND,
    sheet: CellStore,
    /// 行ごとの中身のあるコマ数（シートを測った値。空の行は代わりの行で再生する）
    layout: SheetLayout,
    /// 待機と睡眠の呼吸の大きさ（0 なら付けない）と、いま描いている呼吸の段階
    breathe: f32,
    breath: u32,
    /// シートの 1 コマの画素数（読み込み後に縮めた大きさ）と、元のシートの 1 コマ
    cell: (u32, u32),
    source_cell: (u32, u32),
    /// いま表示しているペット
    pet: PetEntry,
    /// 次に表示したいペット（設定画面で選んだもの）。読み込みが終わると `pet` になる
    want: PetEntry,
    /// シートを別スレッドで読み込み中（同時に 1 本だけ。終わったら `kick_load` で要るか見直す）
    loading: bool,
    /// 別スレッドの読み込みから、結果をメインスレッドへ戻すため
    app: tauri::AppHandle,
    state: AppState,
    /// いま描いている画面の DPI。
    dpi: u32,
    rect: Rect,
    playback: Playback,
    frame: u32,
    epoch: Instant,
    drag: Option<Drag>,
    /// カーソルが乗ってバーまでの待ちを始めたか（TIMER_HOVER が動いている）
    hover_armed: bool,
    /// (行, コマ, 呼吸の段階) → BGRA 事前乗算済みの画素。大きさが変わったら捨てる。
    cache: HashMap<(u32, u32, u32), Vec<u8>>,
    behavior: Behavior,
    pose: Pose,
    /// バー・メニューが出ていると Behavior に伝えた値
    held: bool,
    last_tick_ms: u64,
    /// 散歩で動かす物理ピクセルの端数（1px 未満を次の tick へ持ち越す）
    walk_carry: f32,
    /// 動画クリップ（あるペットだけ。無ければシートで描く）
    clips: Option<Clips>,
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
fn render_cell(sheet: &CellStore, row: u32, frame: u32, w: u32, h: u32) -> Vec<u8> {
    render_bgra(sheet.cell_image(row, frame), w, h)
}

/// クリップの読み込み要求の番号。Clips を作り直しても重ならないよう、プロセス全体で数える。
fn next_clip_request() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// 動画クリップで描くペットの状態（`pet.json` に `clips` があるペットだけ）。
struct Clips {
    pack: ClipPack,
    player: Player,
    cache: ClipCache,
    /// 別スレッドで読んでいる要求の番号（同時に 1 本だけ。番号は `next_clip_request` でプロセス全体で一意）
    loading: Option<u64>,
    /// いま描いているもの
    show: Option<Show>,
    /// 最後に描いた画素（クリップを読み終える前に窓を動かす・描き直すときに出す）
    last: Option<((u32, u32), Vec<u8>)>,
    /// 止めたコマに呼吸を付けた絵（呼吸の段階 → 画素）。止めるコマが変わったら捨てる
    hold: HashMap<u32, Vec<u8>>,
    /// 読めなかったクリップ（読み直さない。シートを読み直したら空にする）
    failed: Vec<usize>,
}

impl Clips {
    fn new(pack: ClipPack, pose: Pose, now: u64) -> Self {
        let player = Player::new(pack.set.clone(), pose, now);
        // 待機とつまみ中のループは捨てない（clipstore::ClipCache::pinned）
        let pinned = pack
            .set
            .clips
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                c.kind == ClipKind::Loop && matches!(c.from.still, Still::Idle | Still::Dragged)
            })
            .map(|(i, _)| i)
            .collect();
        Self {
            pack,
            player,
            cache: ClipCache::with_pinned(pinned),
            loading: None,
            show: None,
            last: None,
            hold: HashMap::new(),
            failed: Vec::new(),
        }
    }

    /// 眠るつなぎの最後のコマで止まっている（呼吸を付ける）か。
    fn holding_sleep(&self) -> bool {
        match self.show {
            Some(Show::Clip { index, frame, .. }) => {
                let c = &self.pack.set.clips[index];
                c.kind == ClipKind::Transition
                    && c.to.still == Still::Sleep
                    && frame + 1 == c.frames
            }
            _ => false,
        }
    }
}

/// 呼吸の段階のコマを描く。`scale` だけ縦に伸ばして描き、上にはみ出した分を切り落とす
/// （足元を固定したまま伸び縮みする。コマの上は透明な余白なので、切り落としても絵は欠けない）。
fn render_breathing(
    sheet: &CellStore,
    row: u32,
    frame: u32,
    size: (u32, u32),
    scale: f32,
) -> Vec<u8> {
    let (w, h) = size;
    let tall = (h as f32 * scale).round() as u32;
    if tall <= h {
        return render_cell(sheet, row, frame, w, h);
    }
    let full = render_cell(sheet, row, frame, w, tall);
    full[((tall - h) * w * 4) as usize..].to_vec()
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
        let clip = pose.clip_in(&self.layout);
        self.playback = Playback {
            clip,
            started_ms: now,
        };
        self.frame = 0;
        // 描いたコマの保存はいまの行だけにする（全ポーズ分ためると、大きく表示したとき数十 MB になる。
        // 2026-10-21 検証係の指摘。行に戻ったら描き直す。1 コマの縮小は軽い）
        self.cache.retain(|(row, _, _), _| *row == clip.row);
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
        let mut dirty = false;
        let mut may_move = true;
        // クリップがあるペットは、動画のコマとシートの行のどちらで描くかを Player が決める
        let mut sheet_pose = Some(out.pose);
        let mut breath_pose = None;
        let mut from_clip = false;
        if let Some(c) = self.clips.as_mut() {
            c.player.set_pose(out.pose, now);
            let show = c.player.tick(now);
            may_move = c.player.may_move();
            if c.show != Some(show) {
                from_clip = matches!(c.show, Some(Show::Clip { .. }));
                c.show = Some(show);
                c.hold.clear();
                dirty = true;
            }
            sheet_pose = match show {
                Show::Sheet(p) => Some(p),
                Show::Clip { .. } => None,
            };
            if c.holding_sleep() {
                breath_pose = Some(Pose::Sleep);
            }
        }
        if let Some(pose) = sheet_pose {
            if from_clip {
                // 動画からシートに戻ったら、同じポーズでも頭から（前に見せたときの時計を使わない）
                self.pose = pose;
                self.playback = Playback {
                    clip: pose.clip_in(&self.layout),
                    started_ms: now,
                };
                self.frame = 0;
            } else {
                dirty |= self.set_pose(pose, now);
            }
            let (frame, _) = self.playback.frame_at(now);
            if frame != self.frame {
                self.frame = frame;
                dirty = true;
            }
            breath_pose = Some(pose);
        }
        // 呼吸は時刻から描くたびに計算する（コマに焼き込むと 1 周 6 コマの段差になる）
        let breath = match breath_pose {
            Some(p) if self.breathe > 0.0 => breath_level(p, now).unwrap_or(0),
            _ => 0,
        };
        if breath != self.breath {
            if std::env::var_os("PET_TRACE").is_some() {
                log(format!("breath {} -> {breath}", self.breath));
            }
            self.breath = breath;
            dirty = true;
        }
        // 窓を動かすのは歩きのループの間だけ（止まる・歩き出すのつなぎの最中は動かさない）
        if out.vx != 0.0 && self.drag.is_none() && may_move {
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
        if let Some(Show::Clip {
            index,
            frame,
            mirror,
        }) = self.clips.as_ref().and_then(|c| c.show)
        {
            self.present_clip(index, frame, mirror);
            return;
        }
        let (w, h) = self.size();
        let key = (self.playback.clip.row, self.frame, self.breath);
        let sheet = &self.sheet;
        let scale = breath_scale(self.breath, self.breathe);
        let pixels = self
            .cache
            .entry(key)
            .or_insert_with(|| render_breathing(sheet, key.0, key.1, (w as u32, h as u32), scale));
        if let Err(e) = blit(self.hwnd, &self.rect, pixels, w, h) {
            log(format!("UpdateLayeredWindow に失敗: {e}"));
        }
        ui::note_pet(self.rect, self.dpi);
    }

    /// 動画のコマを描く。まだ読めていなければ読み込みを始め、それまでは直前の絵（無ければシートの待機）を出す
    /// （窓の移動は描くことで行うので、何も出さないとつまんだ窓がついて来ない）。
    fn present_clip(&mut self, index: usize, frame: u32, mirror: bool) {
        let (w, h) = self.size();
        let size = (w as u32, h as u32);
        let store = self.clip_store_size();
        // 止めたコマの呼吸は 8 段（偶数の段）で描く。下の hold の鍵（breath / 2）と同じ段から伸び具合を出す
        let breath = self.breath / 2 * 2;
        let scale = breath_scale(breath, self.breathe);
        let Some(c) = self.clips.as_mut() else {
            return;
        };
        let holding_sleep = c.holding_sleep();
        let fresh = match c.cache.get(index) {
            Some(f) if f.size == store => {
                // 元の解像度より大きく描くときだけ、描く瞬間に引き伸ばす（持つ量を元の解像度までに抑える）
                let full = |f: &ClipFrames| fit(f.compose(frame, mirror), store, size);
                Some(if holding_sleep && scale > 1.0 {
                    // 止めたコマの呼吸は窓の大きさの絵を段階ごとに持つので、16 段を 8 段に丸める
                    // （200% の画面で 16 段は 11MB。8 段なら 1 段 0.8px 未満で段差は見えない）
                    c.hold
                        .entry(breath / 2)
                        .or_insert_with(|| stretch_up(&full(f), size, scale))
                        .clone()
                } else {
                    full(f)
                })
            }
            _ => None,
        };
        let loaded = fresh.is_some();
        if let Some(px) = fresh {
            c.last = Some((size, px));
        }
        let fallback;
        let pixels: &[u8] = match c.last.as_ref().filter(|(s, _)| *s == size) {
            Some((_, px)) => px,
            None => {
                fallback = render_cell(
                    &self.sheet,
                    Pose::Idle.clip_in(&self.layout).row,
                    0,
                    size.0,
                    size.1,
                );
                &fallback
            }
        };
        if let Err(e) = blit(self.hwnd, &self.rect, pixels, w, h) {
            log(format!("UpdateLayeredWindow に失敗: {e}"));
        }
        ui::note_pet(self.rect, self.dpi);
        if !loaded {
            self.request_clip(index);
            return;
        }
        // 次に流すつなぎを先に読んでおく（流す番になってから読むと、最初の数コマが飛ぶ）。
        // 無ければ、固定したクリップ（つまみ中など）のまだ読んでいないものを読む
        let next = self.clips.as_ref().and_then(|c| {
            c.player.next_clip().or_else(|| {
                c.cache
                    .pinned
                    .iter()
                    .copied()
                    .find(|&i| !c.cache.contains(i) && !c.failed.contains(&i))
            })
        });
        if let Some(next) = next {
            self.request_clip(next);
        }
    }

    /// クリップのコマを別スレッドで読む（同時に 1 本だけ。読めたら `finish_clip`）。
    fn request_clip(&mut self, index: usize) {
        let size = self.clip_store_size();
        let pet_id = self.pet.id.clone();
        let app = self.app.clone();
        let Some(c) = self.clips.as_mut() else {
            return;
        };
        if c.loading.is_some() || c.cache.contains(index) || c.failed.contains(&index) {
            return;
        }
        let def = c.pack.set.clips[index].clone();
        let dir = c.pack.dir.clone();
        // 読み込む分の場所を先に空ける（切り抜くと窓の 7〜8 割になるので、多めに 8 割で見積もる）
        let playing = match c.show {
            Some(Show::Clip { index, .. }) => Some(index),
            _ => None,
        };
        let need = def.frames as usize * (size.0 * size.1 * 4) as usize * 8 / 10;
        c.cache.make_room(need, playing);
        let request = next_clip_request();
        c.loading = Some(request);
        let spawned = std::thread::Builder::new().spawn(move || {
            let started = Instant::now();
            // 壊れたコマで展開が panic しても「失敗」として戻す（戻さないと loading が立ったままになる）
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ClipFrames::load(&dir, &def.name, def.frames, size)
            }))
            .unwrap_or_else(|_| Err("クリップの読み込みが panic した".into()));
            let ms = started.elapsed().as_millis();
            let _ = app.run_on_main_thread(move || {
                with_pet(move |pet| pet.finish_clip(&pet_id, request, index, ms, result))
            });
        });
        if let Err(e) = spawned {
            c.loading = None;
            log(format!("クリップを読むスレッドを作れない: {e}"));
        }
    }

    /// クリップの読み込みが終わった（メインスレッド）。いまのペットと窓の大きさのものなら持つ。
    fn finish_clip(
        &mut self,
        pet_id: &str,
        request: u64,
        index: usize,
        ms: u128,
        result: Result<ClipFrames, String>,
    ) {
        let store = self.clip_store_size();
        let same_pet = pet_id == self.pet.id;
        let Some(c) = self.clips.as_mut() else {
            return;
        };
        // いまの Clips が待っている要求でなければ捨てる。読んでいる間にペットが替わった、A→B→A と戻って
        // Clips が作り直された、のどちらでも番号が合わない（添字は別のクリップを指しうる。検証係の指摘 2026-10-21）
        if !same_pet || c.loading != Some(request) {
            return;
        }
        c.loading = None;
        let playing = match c.show {
            Some(Show::Clip { index, .. }) => Some(index),
            _ => None,
        };
        match result {
            Ok(f) if same_pet && f.size == store => {
                log(format!(
                    "clip {} loaded in {ms} ms: {} KB (cache {} KB)",
                    c.pack.set.clips[index].name,
                    f.bytes() / 1024,
                    (c.cache.bytes() + f.bytes()) / 1024
                ));
                c.cache.insert(index, f, playing);
            }
            // 読んでいる間にペットか窓の大きさが変わった。次に描くときに読み直す
            Ok(_) => {}
            Err(e) => {
                // 同じクリップを何度も読みに行かない（直前の絵で描き続ける）
                log(format!(
                    "クリップ {} を読めない: {e}",
                    c.pack.set.clips[index].name
                ));
                c.failed.push(index);
            }
        }
        self.present();
    }

    /// クリップのコマを持つ大きさ: 窓の大きさ。ただし元のコマ（シートと同じ `source_cell`）の 3/4 より大きくはしない
    /// （それより大きい窓には描く瞬間に引き伸ばす）。2026-10-21 実測: 元の大きさ（384×416）で持つと、大きさ 105%・
    /// 200% の画面（窓 404×437）で待機 17MB・つまみ中 9MB・歩き 15MB になり、散歩中の専有が 66.7MB と PRD N1（50MB）を
    /// 超えた。3/4（288×312）なら 100% の画面の大きさ 150% と同じ持ち方になり、44MB で収まる
    fn clip_store_size(&self) -> (u32, u32) {
        let (w, h) = self.size();
        let (w, h) = (w as u32, h as u32);
        let cap = pets::max_cell(self.source_cell, true);
        if w > cap.0 || h > cap.1 { cap } else { (w, h) }
    }

    /// 描いた絵の保存を捨てる（窓の大きさが変わった・シートを差し替えた）。
    fn clear_caches(&mut self) {
        self.cache.clear();
        if let Some(c) = self.clips.as_mut() {
            c.cache.clear();
            c.hold.clear();
        }
    }

    /// 窓の大きさを `dpi` に合わせる。足元（下端中央）を基準に置き直す。
    fn apply_dpi(&mut self, dpi: u32) {
        if dpi == self.dpi {
            return;
        }
        let anchor = placement::anchor_of(&self.rect);
        self.dpi = dpi;
        self.clear_caches();
        let (w, h) = self.size();
        self.rect = placement::rect_from_anchor(anchor, w, h);
        log(format!("dpi -> {dpi} size {w}x{h}"));
        self.kick_load();
    }

    /// 読み込みが要るなら別スレッドで始める（同時に 1 本だけ）。要るのは
    /// ①選んだペットがまだ表示されていない ②表示に要る解像度が、持っているシートより細かい
    /// （大きさを上げた・より高い DPI の画面がつながった。元のシートより細かくは読まない）。
    /// 大きさのスライダーを動かすたび・画面が変わるたびに呼んでよい。メインスレッドは止まらず、
    /// 読み込み中に何度呼ばれても、終わったときに最新の選択と大きさで見直す（2026-10-21 検証係の指摘）
    fn kick_load(&mut self) {
        if self.loading {
            return;
        }
        let need = pets::needed_cell(display_factor(self.state.scale));
        let switching = self.want.id != self.pet.id;
        let max = pets::max_cell(self.source_cell, self.clips.is_some());
        let sharper = need.0 > self.cell.0 && self.cell.0 < max.0;
        if !switching && !sharper {
            return;
        }
        self.loading = true;
        let entry = self.want.clone();
        let app = self.app.clone();
        let spawned = std::thread::Builder::new().spawn(move || {
            // 壊れたシートで画像の展開が panic しても「失敗」として戻す。戻さないと loading が立ったままで、
            // 以後の切り替えも読み直しも止まる（2026-10-21 検証係の指摘）
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pets::load(&entry, need)))
                    .unwrap_or_else(|_| Err("シートの読み込みが panic した".into()));
            let _ =
                app.run_on_main_thread(move || with_pet(move |pet| pet.finish_load(entry, result)));
        });
        if let Err(e) = spawned {
            self.loading = false;
            log(format!("シートを読むスレッドを作れない: {e}"));
        }
    }

    /// 別スレッドの読み込みが終わった（メインスレッド）。まだ欲しいペットなら差し替える。
    fn finish_load(&mut self, entry: PetEntry, result: Result<LoadedSheet, String>) {
        self.loading = false;
        match result {
            Ok(loaded) if entry.id == self.want.id => {
                let switched = entry.id != self.pet.id;
                self.use_sheet(loaded);
                self.pet = entry;
                if switched {
                    log(format!("pet -> {}", self.pet.id));
                    self.state.pet = Some(self.pet.id.clone());
                    self.state.save();
                }
                self.present();
            }
            // 読んでいる間に別のペットが選ばれた。下の kick_load で選び直したほうを読む
            Ok(_) => {}
            Err(e) => {
                log(format!("ペット {} を読めない: {e}", entry.id));
                if entry.id == self.pet.id {
                    // 同じペットを細かく読み直せなかった。何度も試さないよう、いまの解像度で止める
                    self.source_cell = self.cell;
                } else if entry.id == self.want.id {
                    // 選んだペットが読めない。選択を表示中のペットに戻す
                    self.want = self.pet.clone();
                }
            }
        }
        self.kick_load();
    }

    /// 読み込んだシートに差し替える。再生中のポーズは時計を保ったまま新しいシートで続ける
    /// （一度きりの反応を、差し替えのたびに見せ直さない）。
    fn use_sheet(&mut self, loaded: LoadedSheet) {
        self.sheet = loaded.cells;
        self.layout = loaded.layout;
        self.breathe = loaded.breathe;
        self.cell = loaded.cell;
        self.source_cell = loaded.source_cell;
        // 同じペットを読み直しただけ（大きさの変更）なら、動画の再生の状態は保つ
        let now = now_ms(self.epoch);
        self.clips = match (loaded.clips, self.clips.take()) {
            (Some(pack), Some(mut c)) if c.pack.dir == pack.dir => {
                c.pack = pack;
                c.failed.clear();
                Some(c)
            }
            (Some(pack), _) => Some(Clips::new(pack, self.pose, now)),
            (None, _) => None,
        };
        self.clear_caches();
        self.playback.clip = self.pose.clip_in(&self.layout);
        self.frame = self.frame.min(self.playback.clip.frames.saturating_sub(1));
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
            self.clear_caches();
        }
        // 画面の構成が変わり、より高い DPI の画面がつながったかもしれない（2026-10-21 検証係の指摘）
        self.kick_load();
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
        // 押したらバーまでの待ちはやめる（クリック・ドラッグが優先）
        self.disarm_hover();
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

    /// ペットの不透明な部分にカーソルが乗った（WM_MOUSEMOVE は不透明な画素の上でだけ来る）。
    /// 少し待ってから、まだ乗っていればバーを出す。
    fn arm_hover(&mut self) {
        if self.hover_armed {
            return;
        }
        self.hover_armed = true;
        // SAFETY: 自分の窓へのタイマー設定
        unsafe { SetTimer(Some(self.hwnd), TIMER_HOVER, HOVER_DWELL_MS, None) };
    }

    fn disarm_hover(&mut self) {
        if self.hover_armed {
            self.hover_armed = false;
            // SAFETY: 自分の窓のタイマーを止める
            let _ = unsafe { KillTimer(Some(self.hwnd), TIMER_HOVER) };
        }
    }

    fn on_hover_timer(&mut self) {
        self.disarm_hover();
        if self.drag.is_some() {
            return;
        }
        // 待っている間に離れていたら出さない（離れると WM_MOUSEMOVE が来ないので、ここで確かめる）
        let mut p = POINT::default();
        // SAFETY: 出力先はローカル変数
        if unsafe { GetCursorPos(&mut p) }.is_err() {
            return;
        }
        // SAFETY: 読み取りのみ
        if unsafe { WindowFromPoint(p) } != self.hwnd {
            return;
        }
        ui::show_bar_on_hover(self.rect, self.dpi);
    }

    fn on_mouse_move(&mut self) {
        let c = message_cursor();
        let Some(drag) = self.drag.as_mut() else {
            self.arm_hover();
            return;
        };
        if !drag.moved {
            let (dx, dy) = (c.x - drag.down_cursor.x, c.y - drag.down_cursor.y);
            if dx.abs() < DRAG_THRESHOLD_PX && dy.abs() < DRAG_THRESHOLD_PX {
                return;
            }
            drag.moved = true;
            log("drag start");
            // 借用中なので後回しにする（UI の窓に触る処理はすべて defer を通す）。
            // チャット欄は閉じず、ペットについて行く（ui::closes_on）
            ui::defer(|| ui::close(ui::Close::Drag));
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
        // バー・チャット欄・メニューが出ている間は取り直さない。チャット欄の答えの島はペットの上に
        // 伸びるので、取り直すとペットが答えの文を隠した（2026-10-22 実機確認で発見）
        if ui::popups_visible() {
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
/// 窓に描く DIB とそれを選んだメモリ DC。作り直さずに使い回し、窓の大きさが変わったときだけ作り直す。
///
/// 以前は描くたびに GetDC・CreateCompatibleDC・CreateDIBSection（窓の大きさの確保とゼロ埋め）・解放をしていた。
/// 動画のループは 1 秒に 8 回描くので、大きさ 105%・200% の画面（窓 403×437）で待機の CPU が 2.81% と
/// PRD N1（2%）を超えた（2026-10-21 実測。動画なしの版も呼吸の描き直しで同条件 2.34%）
struct Surface {
    dc: HDC,
    dib: HBITMAP,
    /// DIB を選ぶ前に DC が持っていたもの（解放の前に戻す）
    old: HGDIOBJ,
    bits: *mut u8,
    size: (i32, i32),
}

impl Surface {
    fn new(w: i32, h: i32) -> windows::core::Result<Self> {
        // SAFETY: 画面互換のメモリ DC と、上から下へ並ぶ 32bit の DIB を作る。失敗したら作ったものを解放して返す
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return Err(windows::core::Error::from_thread());
            }
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
            let dib = match CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(dib) => dib,
                Err(e) => {
                    let _ = DeleteDC(dc);
                    return Err(e);
                }
            };
            let old = SelectObject(dc, HGDIOBJ(dib.0));
            if old.is_invalid() {
                // 選べなかった DIB のまま保存すると、同じ大きさでは作り直さず描けないままになる（検証係の指摘）
                let _ = DeleteObject(HGDIOBJ(dib.0));
                let _ = DeleteDC(dc);
                return Err(windows::core::Error::from_thread());
            }
            Ok(Self {
                dc,
                dib,
                old,
                bits: bits.cast::<u8>(),
                size: (w, h),
            })
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: new で作った DC と DIB。DIB を外してから消す
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(HGDIOBJ(self.dib.0));
            let _ = DeleteDC(self.dc);
        }
    }
}

thread_local! {
    /// 窓は 1 つなので、描く面も 1 つ（メインスレッドだけで使う）
    static SURFACE: RefCell<Option<Surface>> = const { RefCell::new(None) };
}

/// 窓 w×h の BGRA のバイト数。0 以下の大きさと桁あふれは None（i32 のまま掛けると大きな窓で panic する。検証係の指摘）。
fn frame_bytes(w: i32, h: i32) -> Option<usize> {
    if w <= 0 || h <= 0 {
        return None;
    }
    usize::try_from(w)
        .ok()?
        .checked_mul(usize::try_from(h).ok()?)?
        .checked_mul(4)
}

fn blit(hwnd: HWND, rect: &Rect, pixels: &[u8], w: i32, h: i32) -> windows::core::Result<()> {
    if frame_bytes(w, h) != Some(pixels.len()) {
        // 大きさと画素の数が合わないまま写すと、DIB の外へ書いてしまう
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x8007_0057_u32 as i32), // E_INVALIDARG
            format!(
                "画素の数が窓の大きさと合わない: {}x{} に {} バイト",
                w,
                h,
                pixels.len()
            ),
        ));
    }
    SURFACE.with(|cell| {
        let mut surface = cell.borrow_mut();
        if surface.as_ref().map(|s| s.size) != Some((w, h)) {
            // 古い面を先に解放してから作る（窓の大きさの DIB を 2 枚同時に持たない）
            *surface = None;
            *surface = Some(Surface::new(w, h)?);
        }
        let s = surface.as_ref().expect("直前で作った");
        // SAFETY: bits は w*h*4 バイトの DIB（大きさは上で確かめた）。DC は DIB を選んだまま
        unsafe {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), s.bits, pixels.len());
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
            // 転送先の DC は None（既定のパレット）でよい。毎回 GetDC しない
            UpdateLayeredWindow(
                hwnd,
                None,
                Some(&pos),
                Some(&size),
                Some(s.dc),
                Some(&src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        }
    })
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
            WM_RBUTTONUP => {
                pet.disarm_hover();
                ui::show_menu(message_cursor(), pet.rect, pet.dpi);
            }
            WM_TIMER => match wp.0 {
                TIMER_TICK => pet.tick(),
                TIMER_HOVER => pet.on_hover_timer(),
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
pub fn create(
    loaded: LoadedSheet,
    pet: PetEntry,
    state: AppState,
    app: tauri::AppHandle,
) -> windows::core::Result<()> {
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
    let layout = loaded.layout;
    let walk = state.walk;
    let epoch = Instant::now();
    let clips = loaded.clips.map(|pack| Clips::new(pack, Pose::Idle, 0));
    let mut pet = Pet {
        hwnd,
        sheet: loaded.cells,
        layout,
        breathe: loaded.breathe,
        breath: 0,
        cell: loaded.cell,
        source_cell: loaded.source_cell,
        want: pet.clone(),
        pet,
        loading: false,
        app,
        state,
        dpi: 0,
        rect: Rect::from_xywh(0, 0, 1, 1),
        playback: Playback {
            clip: Pose::Idle.clip_in(&layout),
            started_ms: 0,
        },
        frame: 0,
        epoch,
        drag: None,
        hover_armed: false,
        cache: HashMap::new(),
        behavior: Behavior::new(behavior_config(walk), seed(), 0),
        pose: Pose::Idle,
        held: false,
        last_tick_ms: 0,
        walk_carry: 0.0,
        clips,
    };
    pet.relayout("startup");
    // SAFETY: 自分の窓の表示とタイマー
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(Some(hwnd), TIMER_TICK, TICK_MS, None);
        SetTimer(Some(hwnd), TIMER_TOPMOST, 1000, None);
    }
    log(format!("pet window created rows={}", layout.rows()));
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
                let pets: Vec<_> = pets::discover()
                    .into_iter()
                    .map(|e| serde_json::json!({ "id": e.id, "name": e.name }))
                    .collect();
                serde_json::json!({
                    "scale": p.state.scale, "tts": p.state.tts, "wake": p.state.wake, "walk": p.state.walk,
                    "pet": p.want.id, "pets": pets,
                    // 鍵は返さない。保存されているかだけ（N4）
                    "chatAuth": p.state.chat_auth.as_str(), "hasApiKey": crate::cred::has_key(),
                })
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
            pet.clear_caches();
            let (w, h) = pet.size();
            let mut rect = placement::rect_from_anchor(anchor, w, h);
            if let Some(m) = monitors::at_point(rect.center()) {
                rect = placement::clamp_into(&rect, &m.work);
            }
            pet.place(rect);
            log(format!("scale -> {scale:.2} size {w}x{h}"));
            pet.kick_load();
        }
        if save {
            pet.state.save();
        }
    });
}

/// 設定画面でペットを切り替える（K 指示 2026-10-21）。見つからないペットなら何もしない。
/// 読み込み（1〜2 秒）は `kick_load` が別スレッドで行い、終わったら足元の位置と大きさはそのまま絵だけを
/// 差し替えて state.json に残す。続けて選び直しても、最後に選んだものが表示される。
pub fn set_pet(id: String) {
    let Some(entry) = pets::discover().into_iter().find(|e| e.id == id) else {
        log(format!("ペット {id} が見つからない"));
        return;
    };
    with_pet(move |pet| {
        pet.want = entry;
        pet.kick_load();
    });
}

/// 表示倍率の最大（大きさ × つながっている画面の最大 DPI / 96）。画面をまたいでも読み直さずに済むよう最大で見る。
pub fn display_factor(scale: f32) -> f32 {
    let max_dpi = monitors::enumerate()
        .iter()
        .map(|m| m.dpi)
        .max()
        .unwrap_or(96);
    scale * max_dpi as f32 / 96.0
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

/// チャットの返答待ちの間、ペットを作業中の動きにする（hooks の Prompt と同じ扱い。セッション名で区別）。
pub fn chat_thinking(session: &'static str) {
    with_pet(move |pet| {
        let now = now_ms(pet.epoch);
        pet.behavior.on_agent(
            AgentEvent::Prompt {
                session: session.to_string(),
            },
            now,
        );
    });
}

/// チャットの返答が終わった（`ok` なら喜ぶ、失敗なら落ち込む）。
pub fn chat_finished(session: &'static str, ok: bool) {
    with_pet(move |pet| {
        let now = now_ms(pet.epoch);
        let session = session.to_string();
        let ev = if ok {
            AgentEvent::Done { session }
        } else {
            AgentEvent::Error { session }
        };
        pet.behavior.on_agent(ev, now);
    });
}

/// チャットの接続（設定）。ペットがまだ無い時は既定のサブスク。
pub fn chat_auth() -> AuthMode {
    PET.with(|cell| {
        cell.try_borrow()
            .ok()
            .and_then(|g| g.as_ref().map(|p| p.state.chat_auth))
    })
    .unwrap_or(AuthMode::Subscription)
}

/// 設定画面でチャットの接続を変えて保存する。
pub fn set_chat_auth(mode: AuthMode) {
    with_pet(move |pet| {
        if pet.state.chat_auth != mode {
            pet.state.chat_auth = mode;
            pet.state.save();
            log(format!("chat_auth -> {}", mode.as_str()));
        }
    });
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
    // 描く面（DC と DIB）も解放する
    SURFACE.with(|cell| cell.borrow_mut().take());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_bytes_rejects_bad_sizes() {
        assert_eq!(frame_bytes(403, 437), Some(403 * 437 * 4));
        assert_eq!(frame_bytes(0, 437), None);
        assert_eq!(frame_bytes(-1, 437), None);
        // i32 で掛けると 2^31 を超える大きさ（壊しにいくケース。2026-10-21 司令塔）
        assert_eq!(
            frame_bytes(32768, 16384),
            usize::try_from(32768u64 * 16384 * 4).ok()
        );
    }
}
