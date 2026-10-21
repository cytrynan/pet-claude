//! 実機検証用の小さな操作ツール（Phase 0 チェックリストを再実行できる形で残す）。
//! DPI 対応のプロセスとして動くので、座標はすべて物理ピクセル。
//!
//! ```text
//! cargo run -p pet-app --example winctl -- find
//! cargo run -p pet-app --example winctl -- fg
//! cargo run -p pet-app --example winctl -- point <x> <y>
//! cargo run -p pet-app --example winctl -- click <x> <y>
//! cargo run -p pet-app --example winctl -- rclick <x> <y>
//! cargo run -p pet-app --example winctl -- drag <x1> <y1> <x2> <y2> [steps]
//! cargo run -p pet-app --example winctl -- focus <class>
//! cargo run -p pet-app --example winctl -- place <class> <x> <y> <w> <h>
//! cargo run -p pet-app --example winctl -- shot <x> <y> <w> <h> <out.png>
//! cargo run -p pet-app --example winctl -- displaychange   # ペットに WM_DISPLAYCHANGE を送る
//! cargo run -p pet-app --example winctl -- down <x> <y> / move <x> <y> / up   # 押したままの状態を作る
//! ```
//! ⚠️ click / drag は本物のマウスを動かす。

use std::ffi::c_void;
use std::thread::sleep;
use std::time::Duration;

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HGDIOBJ, ReleaseDC, SRCCOPY,
    SelectObject,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, SendInput,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GA_ROOT, GWL_EXSTYLE, GetAncestor, GetClassNameW, GetForegroundWindow,
    GetSystemMetrics, GetWindowLongW, GetWindowRect, GetWindowTextW, IsWindowVisible, PostMessageW,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SWP_NOZORDER,
    SetCursorPos, SetForegroundWindow, SetWindowPos, WM_DISPLAYCHANGE, WindowFromPoint,
};
use windows::core::HSTRING;

fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 128];
    // SAFETY: 読み取りのみ
    let n = unsafe { GetClassNameW(h, &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n])
}

fn title_of(h: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: 読み取りのみ
    let n = unsafe { GetWindowTextW(h, &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n])
}

fn rect_of(h: HWND) -> RECT {
    let mut r = RECT::default();
    // SAFETY: 読み取りのみ
    let _ = unsafe { GetWindowRect(h, &mut r) };
    r
}

fn find(class: &str) -> Option<HWND> {
    // SAFETY: 読み取りのみ
    unsafe { FindWindowW(&HSTRING::from(class), None) }
        .ok()
        .filter(|h| !h.is_invalid())
}

fn mouse(flags: MOUSE_EVENT_FLAGS) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    // SAFETY: 1 件の入力を送る
    unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
}

/// 移動・押す・離すを 1 回の SendInput で送る（間に本物のマウスの入力が割り込まない）。
/// 座標は仮想デスクトップ全体を 0..65535 に正規化した絶対座標。
fn click_at(x: i32, y: i32, down: MOUSE_EVENT_FLAGS, up: MOUSE_EVENT_FLAGS) {
    send_at(x, y, &[down, up]);
}

/// 移動と、続く `flags` を 1 回の SendInput で送る。
fn send_at(x: i32, y: i32, flags: &[MOUSE_EVENT_FLAGS]) {
    // SAFETY: 読み取りのみ
    let (vx, vy, vw, vh) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    let nx = (i64::from(x - vx) * 65535 / i64::from((vw - 1).max(1))) as i32;
    let ny = (i64::from(y - vy) * 65535 / i64::from((vh - 1).max(1))) as i32;
    let abs = MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
    let ev = |flags: MOUSE_EVENT_FLAGS| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: nx,
                dy: ny,
                dwFlags: flags | abs,
                ..Default::default()
            },
        },
    };
    let mut inputs = vec![ev(MOUSEEVENTF_MOVE)];
    inputs.extend(flags.iter().map(|f| ev(*f)));
    // SAFETY: まとめて送る
    unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
}

fn move_to(x: i32, y: i32) {
    // SAFETY: カーソル移動
    let _ = unsafe { SetCursorPos(x, y) };
}

fn n(args: &[String], i: usize) -> i32 {
    args.get(i)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("引数 {i} が数値でない"))
}

fn shot(x: i32, y: i32, w: i32, h: i32, out: &str) {
    // SAFETY: GDI オブジェクトはこの関数内で作って解放する
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, HGDIOBJ(bmp.0));
        let _ = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY);
        let mut bmi = BITMAPINFO {
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
        let mut buf = vec![0u8; (w * h * 4) as usize];
        GetDIBits(
            mem,
            bmp,
            0,
            h as u32,
            Some(buf.as_mut_ptr().cast::<c_void>()),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        for px in buf.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
            px[3] = 255;
        }
        image::RgbaImage::from_raw(w as u32, h as u32, buf)
            .expect("サイズ")
            .save(out)
            .expect("保存");
    }
    println!("saved {out}");
}

fn main() {
    // SAFETY: 起動直後に 1 回
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("find") => match find("PetClaudeWindow") {
            Some(h) => {
                let r = rect_of(h);
                // SAFETY: 読み取りのみ
                let ex = unsafe { GetWindowLongW(h, GWL_EXSTYLE) } as u32;
                let visible = unsafe { IsWindowVisible(h) }.as_bool();
                println!(
                    "rect=({},{})-({},{}) size={}x{} exstyle=0x{ex:08x} visible={visible}",
                    r.left,
                    r.top,
                    r.right,
                    r.bottom,
                    r.right - r.left,
                    r.bottom - r.top
                );
            }
            None => println!("not found"),
        },
        Some("fg") => {
            // SAFETY: 読み取りのみ
            let h = unsafe { GetForegroundWindow() };
            println!("fg class={} title={}", class_of(h), title_of(h));
        }
        Some("point") => {
            // SAFETY: 読み取りのみ
            let h = unsafe {
                WindowFromPoint(POINT {
                    x: n(&args, 1),
                    y: n(&args, 2),
                })
            };
            let root = unsafe { GetAncestor(h, GA_ROOT) };
            println!("point class={} root={}", class_of(h), class_of(root));
        }
        Some("click") => {
            click_at(
                n(&args, 1),
                n(&args, 2),
                MOUSEEVENTF_LEFTDOWN,
                MOUSEEVENTF_LEFTUP,
            );
            println!("clicked");
        }
        Some("rclick") => {
            click_at(
                n(&args, 1),
                n(&args, 2),
                MOUSEEVENTF_RIGHTDOWN,
                MOUSEEVENTF_RIGHTUP,
            );
            println!("right-clicked");
        }
        Some("drag") => {
            let (x1, y1, x2, y2) = (n(&args, 1), n(&args, 2), n(&args, 3), n(&args, 4));
            let steps = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(30);
            move_to(x1, y1);
            sleep(Duration::from_millis(80));
            mouse(MOUSEEVENTF_LEFTDOWN);
            for i in 1..=steps {
                let t = f64::from(i) / f64::from(steps);
                move_to(
                    x1 + (f64::from(x2 - x1) * t) as i32,
                    y1 + (f64::from(y2 - y1) * t) as i32,
                );
                sleep(Duration::from_millis(16));
            }
            sleep(Duration::from_millis(80));
            mouse(MOUSEEVENTF_LEFTUP);
            println!("dragged");
        }
        Some("down") => {
            // 移動と押し下げを 1 回で送る（間に本物のマウスが割り込まない）
            send_at(n(&args, 1), n(&args, 2), &[MOUSEEVENTF_LEFTDOWN]);
            println!("down");
        }
        Some("move") => {
            move_to(n(&args, 1), n(&args, 2));
            println!("moved");
        }
        Some("up") => {
            mouse(MOUSEEVENTF_LEFTUP);
            println!("up");
        }
        Some("focus") => match find(&args[1]) {
            // SAFETY: 前面化の要求のみ
            Some(h) => println!("focus={}", unsafe { SetForegroundWindow(h) }.as_bool()),
            None => println!("not found"),
        },
        Some("place") => match find(&args[1]) {
            Some(h) => {
                // SAFETY: 指定の窓を動かす
                let r = unsafe {
                    SetWindowPos(
                        h,
                        None,
                        n(&args, 2),
                        n(&args, 3),
                        n(&args, 4),
                        n(&args, 5),
                        SWP_NOZORDER,
                    )
                };
                println!("placed={}", r.is_ok());
            }
            None => println!("not found"),
        },
        Some("displaychange") => match find("PetClaudeWindow") {
            Some(h) => {
                // SAFETY: ペットの窓へ通知を送るだけ
                let _ = unsafe { PostMessageW(Some(h), WM_DISPLAYCHANGE, WPARAM(32), LPARAM(0)) };
                println!("posted WM_DISPLAYCHANGE");
            }
            None => println!("not found"),
        },
        Some("shot") => shot(n(&args, 1), n(&args, 2), n(&args, 3), n(&args, 4), &args[5]),
        _ => eprintln!("usage: winctl find|fg|point|click|rclick|drag|focus|place|shot ..."),
    }
}
