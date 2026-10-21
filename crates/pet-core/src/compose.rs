//! 状態ごとの静止画から、スプライトシート（`sprite` の規約）を組み立てる。
//!
//! 絵描きツールや画像生成で作った素材は「1 状態 1 枚の静止画」で、大きさも足元の位置も
//! ばらばらなことが多い（例: まめドラゴンは 1312×1199 が 7 枚、つままれだけ 1199×1312）。
//! ここで次のようにそろえる:
//!
//! 1. 各画像の不透明な範囲（`alpha_bbox`）を測る
//! 2. 基準の画像（待機）の高さがコマの高さの `FILL_HEIGHT` になる倍率を、全部の画像に同じく掛ける
//!    （ポーズが変わってもキャラの大きさが変わらないように）。コマに入らない画像だけさらに縮める
//! 3. 足元（不透明な範囲の下端の中央）をコマの下端の中央に合わせる
//! 4. コマごとに小さな動き（呼吸・上下の揺れ・跳ねる・左右の揺れ）を付けて、静止画でも生きて見せる
//!
//! 縮小は事前乗算してから行う（透明な黒と補間して縁が黒ずむのを防ぐ）。

use image::imageops::{self, FilterType};
use image::{Rgba, RgbaImage};

use crate::sprite::{COLUMNS, Pose};

/// この値以下のアルファは「透明」とみなして不透明な範囲から外す（縁のかすかなにじみを拾わない）。
pub const ALPHA_THRESHOLD: u8 = 8;
/// 基準の画像の高さを、コマの高さのこの割合にする（上下に少し余白を残す）。
pub const FILL_HEIGHT: f32 = 0.86;
/// 足元とコマの下端の間（コマの高さに対する割合）。ペットの窓の下端にバーが重なる分を残す。
pub const FOOT_MARGIN: f32 = 0.03;
/// 1 枚の画像がコマに収まるかの余白（幅・高さに対する割合）。
const FIT_MARGIN: f32 = 0.96;

/// 不透明な範囲 (x, y, 幅, 高さ)。全部透明なら None。
pub fn alpha_bbox(img: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] > ALPHA_THRESHOLD {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x0 != u32::MAX).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

/// コマごとの小さな動き。`amount` は基準の大きさに対する割合（呼吸）か、コマの高さに対する割合（移動）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    /// 動かない
    Still,
    /// 足元を固定して縦に伸び縮みする（待機・睡眠）
    Breathe(f32),
    /// 上下に揺れる（歩く・作業）
    Bob(f32),
    /// 上へ跳ねる（喜ぶ）
    Hop(f32),
    /// 左右に揺れる（困る・つままれ・呼びかけ）
    Sway(f32),
}

impl Motion {
    /// `frame` / `frames` コマ目の (横のずれ, 縦のずれ, 縦の倍率)。ずれはコマの高さに対する割合。
    pub fn at(self, frame: u32, frames: u32) -> (f32, f32, f32) {
        let t = frame as f32 / frames.max(1) as f32;
        let wave = (t * std::f32::consts::TAU).sin();
        match self {
            Self::Still => (0.0, 0.0, 1.0),
            Self::Breathe(a) => (0.0, 0.0, 1.0 + a * (wave + 1.0) / 2.0),
            Self::Bob(a) => (0.0, -a * wave.abs(), 1.0),
            Self::Hop(a) => (0.0, -a * (t * std::f32::consts::PI).sin(), 1.0),
            Self::Sway(a) => (a * wave, 0.0, 1.0),
        }
    }
}

/// シートの 1 行の作り方。
pub struct RowSpec<'a> {
    pub pose: Pose,
    /// その状態の静止画。`animation` があるときは、キャラの大きさをこの絵に合わせるためだけに使う
    pub image: &'a RgbaImage,
    /// 左右反転する（右向きの絵から左向きを作る）
    pub mirror: bool,
    pub motion: Motion,
    /// 描いたアニメーションのコマ（同じ大きさの画像を順に。8 枚まで）。None なら `image` から作る
    pub animation: Option<&'a [RgbaImage]>,
}

fn premultiply(img: &mut RgbaImage) {
    for p in img.pixels_mut() {
        let a = u16::from(p[3]);
        for c in 0..3 {
            p[c] = ((u16::from(p[c]) * a + 127) / 255) as u8;
        }
    }
}

fn unpremultiply(img: &mut RgbaImage) {
    for p in img.pixels_mut() {
        let a = u16::from(p[3]);
        if a == 0 {
            *p = Rgba([0, 0, 0, 0]);
            continue;
        }
        for c in 0..3 {
            p[c] = ((u16::from(p[c]) * 255 + a / 2) / a).min(255) as u8;
        }
    }
}

/// 事前乗算してから縮小し、戻す（アプリが読み込んだシートを必要な解像度まで縮めるのにも使う）。
pub fn resize_clean(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let mut src = img.clone();
    premultiply(&mut src);
    let mut out = imageops::resize(&src, w.max(1), h.max(1), FilterType::Lanczos3);
    unpremultiply(&mut out);
    out
}

/// 不透明な範囲で切り抜いた画像。全部透明なら 1×1 の透明。
fn trim(img: &RgbaImage) -> RgbaImage {
    match alpha_bbox(img) {
        Some((x, y, w, h)) => imageops::crop_imm(img, x, y, w, h).to_image(),
        None => RgbaImage::new(1, 1),
    }
}

/// 全部の画像に掛ける共通の倍率（`reference` の高さがコマの `FILL_HEIGHT` になる）。
pub fn common_scale(reference: &RgbaImage, cell_h: u32) -> f32 {
    let h = alpha_bbox(reference).map_or(1, |b| b.3);
    cell_h as f32 * FILL_HEIGHT / h as f32
}

/// 1 枚をコマの大きさに合わせた倍率（共通の倍率で入らなければ縮める）。
fn fit_scale(trimmed: &RgbaImage, scale: f32, cell_w: u32, cell_h: u32) -> f32 {
    let (w, h) = (trimmed.width() as f32, trimmed.height() as f32);
    let room_h = cell_h as f32 * (1.0 - FOOT_MARGIN) * FIT_MARGIN;
    let room_w = cell_w as f32 * FIT_MARGIN;
    scale.min(room_w / w).min(room_h / h)
}

/// コマ 1 つを描く。足元（下端の中央）をコマの下端の中央から `FOOT_MARGIN` 上に置く。
fn draw_cell(base: &RgbaImage, motion: (f32, f32, f32), cell_w: u32, cell_h: u32) -> RgbaImage {
    let (dx, dy, sy) = motion;
    let img = if (sy - 1.0).abs() > f32::EPSILON {
        let h = (base.height() as f32 * sy).round() as u32;
        resize_clean(base, base.width(), h)
    } else {
        base.clone()
    };
    let mut cell = RgbaImage::new(cell_w, cell_h);
    let foot_y = cell_h as f32 * (1.0 - FOOT_MARGIN) + dy * cell_h as f32;
    let x = (cell_w as f32 / 2.0 - img.width() as f32 / 2.0 + dx * cell_h as f32).round() as i64;
    let y = (foot_y - img.height() as f32).round() as i64;
    imageops::overlay(&mut cell, &img, x, y);
    cell
}

/// 描いたアニメーションのコマを 1 行に並べる。
///
/// 全部のコマの不透明な範囲を合わせた枠で同じように切り出す（コマごとに切り出すと、足の出し入れで
/// 中心がずれて揺れる）。キャラの高さは同じ状態の静止画（`still_h`）に合わせる。アニメーションは
/// 静止画と描いた解像度が違うことが多い（まめドラゴンの歩きは 448×448 のコマで、静止画は 1312×1199）。
fn draw_animation(
    sheet: &mut RgbaImage,
    spec: &RowSpec<'_>,
    frames: &[RgbaImage],
    still_h: f32,
    row: u32,
    (cell_w, cell_h): (u32, u32),
) {
    let union =
        frames
            .iter()
            .filter_map(alpha_bbox)
            .fold(None, |acc: Option<(u32, u32, u32, u32)>, b| {
                let (x0, y0, x1, y1) = (b.0, b.1, b.0 + b.2, b.1 + b.3);
                Some(match acc {
                    None => (x0, y0, x1, y1),
                    Some((a0, b0, a1, b1)) => (a0.min(x0), b0.min(y0), a1.max(x1), b1.max(y1)),
                })
            });
    let Some((x0, y0, x1, y1)) = union else {
        return;
    };
    let (uw, uh) = (x1 - x0, y1 - y0);
    let room_h = cell_h as f32 * (1.0 - FOOT_MARGIN) * FIT_MARGIN;
    let room_w = cell_w as f32 * FIT_MARGIN;
    let s = (still_h / uh as f32)
        .min(room_w / uw as f32)
        .min(room_h / uh as f32);
    let (w, h) = (
        (uw as f32 * s).round() as u32,
        (uh as f32 * s).round() as u32,
    );
    let n = (frames.len() as u32).min(COLUMNS);
    for (i, frame) in frames.iter().take(n as usize).enumerate() {
        // 枠が画像からはみ出す（大きさの違うコマが混じった）ときは、その画像に収まる分だけ切る
        let cw = uw.min(frame.width().saturating_sub(x0));
        let ch = uh.min(frame.height().saturating_sub(y0));
        let mut crop = imageops::crop_imm(frame, x0, y0, cw, ch).to_image();
        if spec.mirror {
            crop = imageops::flip_horizontal(&crop);
        }
        let base = resize_clean(&crop, w, h);
        let cell = draw_cell(&base, spec.motion.at(i as u32, n), cell_w, cell_h);
        imageops::replace(
            sheet,
            &cell,
            i64::from(i as u32 * cell_w),
            i64::from(row * cell_h),
        );
    }
}

/// シートを組み立てる。行数は `rows`（`Pose::native_clip` の行番号まで）、1 コマ `cell_w`×`cell_h`。
/// `reference` は大きさの基準（待機の絵）。`specs` に無い行は透明のまま。
pub fn compose_sheet(
    specs: &[RowSpec<'_>],
    reference: &RgbaImage,
    rows: u32,
    cell_w: u32,
    cell_h: u32,
) -> RgbaImage {
    let scale = common_scale(reference, cell_h);
    let mut sheet = RgbaImage::new(cell_w * COLUMNS, cell_h * rows);
    for spec in specs {
        let clip = spec.pose.native_clip();
        if clip.row >= rows {
            continue;
        }
        let mut trimmed = trim(spec.image);
        if spec.mirror {
            trimmed = imageops::flip_horizontal(&trimmed);
        }
        let s = fit_scale(&trimmed, scale, cell_w, cell_h);
        if let Some(frames) = spec.animation.filter(|f| !f.is_empty()) {
            let still_h = trimmed.height() as f32 * s;
            draw_animation(
                &mut sheet,
                spec,
                frames,
                still_h,
                clip.row,
                (cell_w, cell_h),
            );
            continue;
        }
        let w = (trimmed.width() as f32 * s).round() as u32;
        let h = (trimmed.height() as f32 * s).round() as u32;
        let base = resize_clean(&trimmed, w, h);
        // 動かない行は 1 コマだけ書く（同じ絵を並べても、アプリは行のコマ数を数えて 1 コマの静止画として
        // 再生する。描いた絵の保存も 1 枚で済む。2026-10-21: 待機に呼吸の 16 段階を付けたら、同じ絵 6 コマ分の
        // 保存で専有メモリが 16MB 増えた）
        let frames = if spec.motion == Motion::Still {
            1
        } else {
            clip.frames.min(COLUMNS)
        };
        for frame in 0..frames {
            let cell = draw_cell(&base, spec.motion.at(frame, clip.frames), cell_w, cell_h);
            imageops::replace(
                &mut sheet,
                &cell,
                i64::from(frame * cell_w),
                i64::from(clip.row * cell_h),
            );
        }
    }
    sheet
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 幅 w・高さ h の不透明な長方形を、(x, y) に置いた画像。
    fn blob(canvas: (u32, u32), x: u32, y: u32, w: u32, h: u32) -> RgbaImage {
        let mut img = RgbaImage::new(canvas.0, canvas.1);
        for yy in y..y + h {
            for xx in x..x + w {
                img.put_pixel(xx, yy, Rgba([200, 100, 50, 255]));
            }
        }
        img
    }

    fn cell(sheet: &RgbaImage, row: u32, frame: u32, w: u32, h: u32) -> RgbaImage {
        imageops::crop_imm(sheet, frame * w, row * h, w, h).to_image()
    }

    #[test]
    fn bbox_ignores_faint_edges() {
        let mut img = blob((50, 40), 10, 5, 20, 30);
        img.put_pixel(0, 0, Rgba([0, 0, 0, ALPHA_THRESHOLD]));
        assert_eq!(alpha_bbox(&img), Some((10, 5, 20, 30)));
        assert_eq!(alpha_bbox(&RgbaImage::new(4, 4)), None);
    }

    #[test]
    fn every_pose_gets_the_same_size_and_stands_on_the_floor() {
        // 大きさも置き場所も違う 2 枚が、同じ倍率で縮み、足元がコマの下端にそろう
        let a = blob((400, 300), 50, 20, 100, 200);
        let b = blob((300, 400), 150, 180, 100, 200);
        let specs = [
            RowSpec {
                pose: Pose::Idle,
                image: &a,
                mirror: false,
                motion: Motion::Still,
                animation: None,
            },
            RowSpec {
                pose: Pose::Waiting,
                image: &b,
                mirror: false,
                motion: Motion::Still,
                animation: None,
            },
        ];
        let (cw, ch) = (96, 104);
        let sheet = compose_sheet(&specs, &a, 11, cw, ch);
        assert_eq!(sheet.dimensions(), (cw * COLUMNS, ch * 11));
        let ba = alpha_bbox(&cell(&sheet, 0, 0, cw, ch)).unwrap();
        let bb = alpha_bbox(&cell(&sheet, 6, 0, cw, ch)).unwrap();
        // 高さは FILL_HEIGHT（89px）前後で、2 枚とも同じ
        assert!((ba.3 as i32 - 89).abs() <= 1, "{ba:?}");
        assert_eq!((ba.2, ba.3), (bb.2, bb.3));
        // 足元は下端から FOOT_MARGIN（3px）上・横は中央
        let foot = ch - (ch as f32 * FOOT_MARGIN).round() as u32;
        assert!((ba.1 + ba.3).abs_diff(foot) <= 1, "{ba:?}");
        assert!((ba.0 + ba.2 / 2).abs_diff(cw / 2) <= 1, "{ba:?}");
        // 使わない行は透明
        assert_eq!(alpha_bbox(&cell(&sheet, 3, 0, cw, ch)), None);
    }

    #[test]
    fn too_big_pose_is_shrunk_to_fit() {
        // 基準より 2 倍高い絵（つままれて縦に伸びた絵など）もコマからはみ出さない
        let a = blob((100, 100), 0, 0, 50, 50);
        let tall = blob((100, 200), 0, 0, 50, 100);
        let specs = [RowSpec {
            pose: Pose::Grabbed,
            image: &tall,
            mirror: false,
            motion: Motion::Still,
            animation: None,
        }];
        let sheet = compose_sheet(&specs, &a, 11, 96, 104);
        let b = alpha_bbox(&cell(&sheet, 10, 0, 96, 104)).unwrap();
        assert!(b.1 > 0 && b.3 < 104, "{b:?}");
    }

    #[test]
    fn mirror_flips_left_and_right() {
        // 右半分だけ色が付いた絵を反転すると、色が左に来る
        let mut img = blob((40, 40), 0, 0, 40, 40);
        for y in 0..40 {
            for x in 0..20 {
                img.put_pixel(x, y, Rgba([0, 0, 255, 255]));
            }
        }
        let specs = [RowSpec {
            pose: Pose::RunLeft,
            image: &img,
            mirror: true,
            motion: Motion::Still,
            animation: None,
        }];
        let sheet = compose_sheet(&specs, &img, 11, 96, 104);
        let c = cell(&sheet, 2, 0, 96, 104);
        let b = alpha_bbox(&c).unwrap();
        let left = c.get_pixel(b.0 + 2, b.1 + b.3 / 2);
        let right = c.get_pixel(b.0 + b.2 - 3, b.1 + b.3 / 2);
        assert!(left[0] > left[2], "左は元の右（赤）: {left:?}");
        assert!(right[2] > right[0], "右は元の左（青）: {right:?}");
    }

    #[test]
    fn motions_move_the_figure_but_keep_the_feet_when_breathing() {
        let a = blob((100, 100), 20, 0, 60, 100);
        let specs = [
            RowSpec {
                pose: Pose::Idle,
                image: &a,
                mirror: false,
                motion: Motion::Breathe(0.04),
                animation: None,
            },
            RowSpec {
                pose: Pose::Jumping,
                image: &a,
                mirror: false,
                motion: Motion::Hop(0.1),
                animation: None,
            },
        ];
        let sheet = compose_sheet(&specs, &a, 11, 96, 104);
        let frames: Vec<_> = (0..6)
            .map(|f| alpha_bbox(&cell(&sheet, 0, f, 96, 104)).unwrap())
            .collect();
        // 呼吸: 高さは変わるが、足元（下端）は同じ
        assert!(frames.iter().map(|b| b.3).max() > frames.iter().map(|b| b.3).min());
        let feet: Vec<_> = frames.iter().map(|b| b.1 + b.3).collect();
        assert!(feet.iter().all(|f| f.abs_diff(feet[0]) <= 1), "{feet:?}");
        // 跳ねる: どこかのコマで足元が上がる
        let hop: Vec<_> = (0..5)
            .map(|f| alpha_bbox(&cell(&sheet, 4, f, 96, 104)).unwrap())
            .map(|b| b.1 + b.3)
            .collect();
        assert!(hop.iter().min() < hop.iter().max(), "{hop:?}");
    }

    #[test]
    fn animation_frames_keep_their_relative_motion_and_the_still_size() {
        // 静止画（高さ 200）と、低い解像度で描いたアニメーション 3 コマ（高さ 60・足が左右に動く）
        let still = blob((300, 300), 50, 50, 100, 200);
        let frames = [
            blob((100, 100), 20, 20, 40, 60),
            blob((100, 100), 30, 20, 40, 60),
            blob((100, 100), 40, 20, 40, 60),
        ];
        let specs = [RowSpec {
            pose: Pose::RunRight,
            image: &still,
            mirror: false,
            motion: Motion::Still,
            animation: Some(&frames),
        }];
        let sheet = compose_sheet(&specs, &still, 11, 96, 104);
        let boxes: Vec<_> = (0..3)
            .map(|f| alpha_bbox(&cell(&sheet, 1, f, 96, 104)).unwrap())
            .collect();
        // 3 コマとも書かれ、4 コマ目は空
        assert_eq!(alpha_bbox(&cell(&sheet, 1, 3, 96, 104)), None);
        // キャラの高さは静止画と同じ（FILL_HEIGHT の 89px 前後）。アニメーションの解像度に引きずられない
        let still_row = compose_sheet(
            &[RowSpec {
                pose: Pose::Idle,
                image: &still,
                mirror: false,
                motion: Motion::Still,
                animation: None,
            }],
            &still,
            11,
            96,
            104,
        );
        let sb = alpha_bbox(&cell(&still_row, 0, 0, 96, 104)).unwrap();
        assert!(
            boxes.iter().all(|b| b.3.abs_diff(sb.3) <= 1),
            "{boxes:?} vs {sb:?}"
        );
        // 足元は 3 コマともそろい、横の動き（左→右）は保たれる
        assert!(
            boxes
                .iter()
                .all(|b| (b.1 + b.3).abs_diff(boxes[0].1 + boxes[0].3) <= 1)
        );
        assert!(
            boxes[0].0 < boxes[1].0 && boxes[1].0 < boxes[2].0,
            "{boxes:?}"
        );
        // 反転すると動きも逆になる
        let specs = [RowSpec {
            pose: Pose::RunLeft,
            image: &still,
            mirror: true,
            motion: Motion::Still,
            animation: Some(&frames),
        }];
        let sheet = compose_sheet(&specs, &still, 11, 96, 104);
        let left: Vec<_> = (0..3)
            .map(|f| alpha_bbox(&cell(&sheet, 2, f, 96, 104)).unwrap().0)
            .collect();
        assert!(left[0] > left[1] && left[1] > left[2], "{left:?}");
    }

    #[test]
    fn still_rows_get_a_single_frame() {
        let a = blob((100, 100), 20, 0, 60, 100);
        let specs = [RowSpec {
            pose: Pose::Idle,
            image: &a,
            mirror: false,
            motion: Motion::Still,
            animation: None,
        }];
        let sheet = compose_sheet(&specs, &a, 11, 96, 104);
        assert!(alpha_bbox(&cell(&sheet, 0, 0, 96, 104)).is_some());
        assert_eq!(alpha_bbox(&cell(&sheet, 0, 1, 96, 104)), None);
    }

    #[test]
    fn motion_values() {
        assert_eq!(Motion::Still.at(3, 6), (0.0, 0.0, 1.0));
        let (_, dy, _) = Motion::Hop(0.1).at(0, 6);
        assert!(dy.abs() < 1e-6);
        let (dx, _, _) = Motion::Sway(0.02).at(0, 4);
        assert!(dx.abs() < 1e-6);
    }
}
