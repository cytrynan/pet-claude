//! ディスプレイの列挙（物理ピクセル・Per-Monitor DPI）。

use pet_core::geometry::{Point, Rect};
use pet_core::placement::Monitor;
use windows::Win32::Foundation::{LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromPoint,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::core::BOOL;

const MONITORINFOF_PRIMARY: u32 = 1;

pub const fn to_rect(r: RECT) -> Rect {
    Rect::new(r.left, r.top, r.right, r.bottom)
}

fn describe(hm: HMONITOR) -> Option<Monitor> {
    let mut mi = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: mi は cbSize を設定した有効な MONITORINFO
    if !unsafe { GetMonitorInfoW(hm, &mut mi) }.as_bool() {
        return None;
    }
    let (mut dx, mut dy) = (96u32, 96u32);
    // SAFETY: 出力先はスタック上の u32
    let _ = unsafe { GetDpiForMonitor(hm, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) };
    Some(Monitor {
        bounds: to_rect(mi.rcMonitor),
        work: to_rect(mi.rcWork),
        dpi: dx,
        primary: mi.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}

unsafe extern "system" fn collect(hm: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
    // SAFETY: lp は enumerate() が渡した Vec<Monitor> へのポインタ
    let out = unsafe { &mut *(lp.0 as *mut Vec<Monitor>) };
    if let Some(m) = describe(hm) {
        out.push(m);
    }
    BOOL(1)
}

pub fn enumerate() -> Vec<Monitor> {
    let mut out: Vec<Monitor> = Vec::new();
    // SAFETY: コールバックは同期的に呼ばれ、out はこの関数の間だけ有効であればよい
    let _ =
        unsafe { EnumDisplayMonitors(None, None, Some(collect), LPARAM(&raw mut out as isize)) };
    out
}

/// 点を含む（無ければ最も近い）ディスプレイ。
pub fn at_point(p: Point) -> Option<Monitor> {
    // SAFETY: 引数は値渡しのみ
    let hm = unsafe { MonitorFromPoint(POINT { x: p.x, y: p.y }, MONITOR_DEFAULTTONEAREST) };
    describe(hm)
}
