//! 仮画像の生成。本番素材ができるまで、11 行（Codex 互換 9 行＋睡眠・つままれ）の
//! スプライトシートを手続き的に描く。
//!
//! 検証用の作りにしてある:
//! - 体のまわりは完全に透明（クリックが背後に抜けるかを試す）
//! - 体の縁は半透明のぼかし（縁のヒット判定を試す）
//! - 体に「穴」（透明の窓）を開ける（キャラの内側でもクリックが抜けるかを試す）
//! - 行ごとに色と動きを変える（どのポーズが出ているか見て分かる）

use image::{Rgba, RgbaImage};

use crate::sprite::{CELL_H, CELL_W, COLUMNS, Pose};

/// 生成するシートの行数。
pub const ROWS: u32 = 11;

const POSES: [Pose; ROWS as usize] = [
    Pose::Idle,
    Pose::RunRight,
    Pose::RunLeft,
    Pose::Waving,
    Pose::Jumping,
    Pose::Failed,
    Pose::Waiting,
    Pose::Running,
    Pose::Review,
    Pose::Sleep,
    Pose::Grabbed,
];

fn body_color(pose: Pose) -> [u8; 3] {
    match pose {
        Pose::Idle | Pose::Sleep => [88, 120, 245],
        Pose::RunRight | Pose::RunLeft => [64, 168, 232],
        Pose::Waving | Pose::Jumping | Pose::Petted => [120, 200, 120],
        Pose::Failed => [232, 96, 96],
        Pose::Waiting => [245, 180, 64],
        Pose::Running | Pose::Review => [150, 110, 235],
        Pose::Grabbed => [240, 120, 200],
    }
}

/// 0..1 の被覆率を、縁 `soft` ピクセルでなだらかにした楕円の内側判定。
fn ellipse_coverage(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32, soft: f32) -> f32 {
    let dx = (x - cx) / rx;
    let dy = (y - cy) / ry;
    let d = (dx * dx + dy * dy).sqrt();
    // 縁からの距離をピクセルに直してぼかす
    let edge_px = (1.0 - d) * rx.min(ry);
    (edge_px / soft + 0.5).clamp(0.0, 1.0)
}

fn blend(dst: &mut Rgba<u8>, rgb: [u8; 3], cover: f32) {
    if cover <= 0.0 {
        return;
    }
    let a = (cover * 255.0).round() as u8;
    // 上書き合成（下の色を src-over で重ねる）
    let sa = f32::from(a) / 255.0;
    let da = f32::from(dst[3]) / 255.0;
    let out_a = sa + da * (1.0 - sa);
    for i in 0..3 {
        let s = f32::from(rgb[i]) * sa;
        let d = f32::from(dst[i]) * da * (1.0 - sa);
        dst[i] = if out_a > 0.0 {
            ((s + d) / out_a).round() as u8
        } else {
            0
        };
    }
    dst[3] = (out_a * 255.0).round() as u8;
}

fn draw_cell(img: &mut RgbaImage, row: u32, frame: u32, pose: Pose, frames: u32) {
    let ox = frame * CELL_W;
    let oy = row * CELL_H;
    let t = frame as f32 / frames.max(1) as f32;
    let phase = t * std::f32::consts::TAU;
    let (bob, lean) = match pose {
        Pose::Idle => (phase.sin() * 3.0, 0.0),
        Pose::RunRight => ((phase * 2.0).sin().abs() * -8.0, 10.0),
        Pose::RunLeft => ((phase * 2.0).sin().abs() * -8.0, -10.0),
        Pose::Jumping => (-(phase.sin().max(0.0)) * 30.0, 0.0),
        Pose::Grabbed => (-12.0 + phase.sin() * 4.0, phase.sin() * 6.0),
        Pose::Sleep => (4.0 + phase.sin() * 2.0, 0.0),
        _ => (phase.sin() * 4.0, 0.0),
    };
    let rgb = body_color(pose);
    let cx = CELL_W as f32 / 2.0 + lean;
    let cy = 120.0 + bob;
    let (rx, ry) = (62.0, 70.0);
    let eye_closed = matches!(pose, Pose::Sleep) || (matches!(pose, Pose::Idle) && frame == 3);
    let wave_up = matches!(pose, Pose::Waving | Pose::Waiting) && frame.is_multiple_of(2);

    for y in 0..CELL_H {
        for x in 0..CELL_W {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let px = img.get_pixel_mut(ox + x, oy + y);
            // 体
            blend(px, rgb, ellipse_coverage(fx, fy, cx, cy, rx, ry, 2.5));
            // 手（手を振るポーズで上下）
            let hand_y = if wave_up { cy - 50.0 } else { cy + 10.0 };
            blend(
                px,
                rgb,
                ellipse_coverage(fx, fy, cx + rx - 4.0, hand_y, 14.0, 14.0, 1.5),
            );
            // 足
            blend(
                px,
                rgb,
                ellipse_coverage(fx, fy, cx - 26.0, cy + ry - 4.0, 16.0, 10.0, 1.5),
            );
            blend(
                px,
                rgb,
                ellipse_coverage(fx, fy, cx + 26.0, cy + ry - 4.0, 16.0, 10.0, 1.5),
            );
            // 画面（顔）
            let face = ellipse_coverage(fx, fy, cx, cy - 18.0, 40.0, 26.0, 1.5);
            blend(px, [28, 32, 52], face);
            // 目
            let eye_h = if eye_closed { 2.0 } else { 7.0 };
            for ex in [cx - 15.0, cx + 15.0] {
                blend(
                    px,
                    [120, 230, 255],
                    ellipse_coverage(fx, fy, ex, cy - 18.0, 6.0, eye_h, 1.0),
                );
            }
        }
    }
    // 穴: 体の下寄りに完全透明の四角（キャラの内側でもクリックが抜けるか試す）
    let (hx, hy) = ((cx - 10.0) as u32, (cy + 22.0) as u32);
    for y in hy..hy + 16 {
        for x in hx..hx + 20 {
            if x < CELL_W && y < CELL_H {
                img.put_pixel(ox + x, oy + y, Rgba([0, 0, 0, 0]));
            }
        }
    }
}

/// 仮のスプライトシート（1536×2288）を描く。
pub fn generate_sheet() -> RgbaImage {
    let mut img = RgbaImage::new(CELL_W * COLUMNS, CELL_H * ROWS);
    for (row, pose) in POSES.iter().enumerate() {
        let frames = pose.native_clip().frames;
        for frame in 0..frames {
            draw_cell(&mut img, row as u32, frame, *pose, frames);
        }
    }
    img
}

/// トレイとアプリのアイコン（待機の 1 コマ目を正方形に切り出す）。
pub fn generate_icon(size: u32) -> RgbaImage {
    let sheet = generate_sheet();
    let cell = image::imageops::crop_imm(&sheet, 0, 8, CELL_W, CELL_W).to_image();
    image::imageops::resize(&cell, size, size, image::imageops::FilterType::Triangle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_has_extended_codex_size() {
        let s = generate_sheet();
        assert_eq!((s.width(), s.height()), (1536, 2288));
    }

    #[test]
    fn corners_are_fully_transparent_and_body_is_opaque() {
        let s = generate_sheet();
        for row in 0..ROWS {
            let (ox, oy) = (0, row * CELL_H);
            assert_eq!(s.get_pixel(ox + 2, oy + 2)[3], 0, "row {row} corner");
            assert_eq!(s.get_pixel(ox + 96, oy + 100)[3], 255, "row {row} body");
        }
    }

    #[test]
    fn hole_inside_body_is_fully_transparent() {
        let s = generate_sheet();
        // 待機 1 コマ目: cy=120 付近、穴は (cx-10, cy+22) から 20x16
        assert_eq!(s.get_pixel(96, 120 + 22 + 8)[3], 0);
    }

    #[test]
    fn body_edge_has_semi_transparent_pixels() {
        let s = generate_sheet();
        let partial = (0..CELL_W)
            .map(|x| s.get_pixel(x, 120)[3])
            .filter(|a| *a > 0 && *a < 255);
        assert!(partial.count() >= 2);
    }

    #[test]
    fn unused_cells_stay_empty() {
        let s = generate_sheet();
        // 待機は 6 コマ。7 列目は空
        assert!((0..CELL_H).all(|y| s.get_pixel(6 * CELL_W + 96, y)[3] == 0));
    }

    #[test]
    fn icon_is_square() {
        let i = generate_icon(64);
        assert_eq!((i.width(), i.height()), (64, 64));
    }
}
