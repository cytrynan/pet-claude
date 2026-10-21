//! 窓に描く画素の下ごしらえ（事前乗算・縮小・切り抜き・左右反転・引き伸ばし）。
//!
//! 画素を 1 つずつ回す処理は、最適化しないビルドだと桁違いに遅い（2026-10-21 実測: pet-app に置いたら
//! 待機の動画 39 コマの読み込みに 3〜4 秒）。pet-core はデバッグビルドでも最適化する
//! （ワークスペースの `[profile.dev.package.pet-core]`）ので、ここに置く。

use image::RgbaImage;

/// この値未満のアルファは 0 に落とす（縁のかすかな影でクリックを拾わない）。
pub const ALPHA_CUTOFF: u8 = 24;

/// 切り抜いた 1 コマ: (左上 x, 左上 y, 幅, 高さ, BGRA)。
pub type Cropped = (u32, u32, u32, u32, Vec<u8>);

/// 事前乗算 → 縮小 → BGRA に並べ替える。縮小の前に事前乗算するのは、透明な黒と補間して縁が黒ずむのを防ぐため。
pub fn render_bgra(mut img: RgbaImage, w: u32, h: u32) -> Vec<u8> {
    for p in img.pixels_mut() {
        let a = u16::from(p[3]);
        for c in 0..3 {
            p[c] = ((u16::from(p[c]) * a + 127) / 255) as u8;
        }
    }
    let scaled = if img.dimensions() == (w, h) {
        img
    } else {
        image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle)
    };
    let mut out = scaled.into_raw();
    for p in out.as_chunks_mut::<4>().0 {
        if p[3] < ALPHA_CUTOFF {
            *p = [0, 0, 0, 0];
        } else {
            p.swap(0, 2);
        }
    }
    out
}

/// アルファが 0 でない範囲だけを切り出す。全部透明なら None。
pub fn crop(bgra: &[u8], (w, h): (u32, u32)) -> Option<Cropped> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..h {
        let row = &bgra[(y * w * 4) as usize..((y + 1) * w * 4) as usize];
        let px = row.as_chunks::<4>().0;
        if let Some(first) = px.iter().position(|p| p[3] != 0) {
            let last = px.iter().rposition(|p| p[3] != 0).unwrap_or(first);
            let (first, last) = (first as u32, last as u32);
            x0 = x0.min(first);
            x1 = x1.max(last);
            y0 = y0.min(y);
            y1 = y1.max(y);
        }
    }
    if x0 == u32::MAX {
        return None;
    }
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut px = Vec::with_capacity((cw * ch * 4) as usize);
    for y in y0..=y1 {
        let s = ((y * w + x0) * 4) as usize;
        px.extend_from_slice(&bgra[s..s + (cw * 4) as usize]);
    }
    Some((x0, y0, cw, ch, px))
}

/// 切り抜いたコマを `size` の透明なキャンバスに戻す。`mirror` なら左右反転する。
pub fn compose(cropped: Option<&Cropped>, (w, h): (u32, u32), mirror: bool) -> Vec<u8> {
    let mut out = vec![0u8; (w * h * 4) as usize];
    let Some((x, y, cw, ch, px)) = cropped else {
        return out;
    };
    for row in 0..*ch {
        let src = &px[(row * cw * 4) as usize..((row + 1) * cw * 4) as usize];
        let dst_row = ((y + row) * w * 4) as usize;
        if mirror {
            // 反転したときの左端は w - (x + cw)。画素の順を逆にして置く
            let left = (w - x - cw) as usize;
            let dst = &mut out[dst_row + left * 4..dst_row + (left + *cw as usize) * 4];
            let src = src.as_chunks::<4>().0;
            for (d, s) in dst.as_chunks_mut::<4>().0.iter_mut().zip(src.iter().rev()) {
                *d = *s;
            }
        } else {
            let d = dst_row + (*x as usize) * 4;
            out[d..d + src.len()].copy_from_slice(src);
        }
    }
    out
}

/// `from` の大きさの BGRA を `to` に引き伸ばす（同じなら何もしない）。事前乗算済みなので補間で縁は黒ずまない。
///
/// 動画のコマを窓に描くたびに呼ぶので軽くする: 整数（1/256 単位）の双線形補間。`image` の汎用の縮小（Triangle）は
/// 拡大にも畳み込みを回し、288×312→404×437 を 1 秒 8 回で待機中の CPU が 2.97% になった（2026-10-21 実測。N1 は 2%）
pub fn fit(px: Vec<u8>, from: (u32, u32), to: (u32, u32)) -> Vec<u8> {
    if from == to {
        return px;
    }
    let (sw, sh) = (from.0 as usize, from.1 as usize);
    let (dw, dh) = (to.0 as usize, to.1 as usize);
    // 出力の画素の中心を元の座標に写す: s = (d + 0.5) × 元 / 先 − 0.5（端は端の画素に寄せる）
    let axis = |d: usize, src: usize, dst: usize| -> (usize, usize, u32) {
        let s = ((d as f32 + 0.5) * src as f32 / dst as f32 - 0.5).clamp(0.0, (src - 1) as f32);
        let i = s.floor() as usize;
        (i, (i + 1).min(src - 1), ((s - i as f32) * 256.0) as u32)
    };
    let xs: Vec<(usize, usize, u32)> = (0..dw).map(|x| axis(x, sw, dw)).collect();
    let mut out = vec![0u8; dw * dh * 4];
    for y in 0..dh {
        let (y0, y1, fy) = axis(y, sh, dh);
        let (r0, r1) = (
            &px[y0 * sw * 4..(y0 + 1) * sw * 4],
            &px[y1 * sw * 4..(y1 + 1) * sw * 4],
        );
        let row = &mut out[y * dw * 4..(y + 1) * dw * 4];
        for (x, &(x0, x1, fx)) in xs.iter().enumerate() {
            for c in 0..4 {
                let top = u32::from(r0[x0 * 4 + c]) * (256 - fx) + u32::from(r0[x1 * 4 + c]) * fx;
                let bottom =
                    u32::from(r1[x0 * 4 + c]) * (256 - fx) + u32::from(r1[x1 * 4 + c]) * fx;
                row[x * 4 + c] = ((top * (256 - fy) + bottom * fy + (1 << 15)) >> 16) as u8;
            }
        }
    }
    out
}

/// 窓いっぱいの BGRA を、足元を固定したまま `scale` だけ縦に伸ばす（上にはみ出した分は捨てる）。
/// 動画の止めたコマ（眠るの最後）に呼吸を付けるため。
pub fn stretch_up(px: &[u8], (w, h): (u32, u32), scale: f32) -> Vec<u8> {
    let tall = (h as f32 * scale).round() as u32;
    if tall <= h {
        return px.to_vec();
    }
    // BGRA を RGBA の器に入れて伸ばす（チャンネルの並びは補間に関係しない）
    let img = RgbaImage::from_raw(w, h, px.to_vec()).expect("窓の大きさの画素");
    let big = image::imageops::resize(&img, w, tall, image::imageops::FilterType::Triangle);
    big.into_raw()[((tall - h) * w * 4) as usize..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_places_and_mirrors_the_cropped_frame() {
        // 4×2 の窓の左上 (0,0) に 2×1 の絵（1 と 2）
        let c: Cropped = (0, 0, 2, 1, vec![1, 1, 1, 255, 2, 2, 2, 255]);
        let plain = compose(Some(&c), (4, 2), false);
        assert_eq!(&plain[0..8], &[1, 1, 1, 255, 2, 2, 2, 255]);
        assert!(plain[8..].iter().all(|&b| b == 0));
        // 反転すると右端に、左右が入れ替わって来る
        let m = compose(Some(&c), (4, 2), true);
        assert_eq!(&m[8..16], &[2, 2, 2, 255, 1, 1, 1, 255]);
        assert!(m[0..8].iter().all(|&b| b == 0));
        assert!(compose(None, (4, 2), false).iter().all(|&b| b == 0));
    }

    #[test]
    fn crop_keeps_only_visible_pixels() {
        let mut bgra = vec![0u8; 3 * 3 * 4];
        bgra[(4 * 4) + 3] = 200; // 中央 (1,1)
        let (x, y, w, h, px) = crop(&bgra, (3, 3)).unwrap();
        assert_eq!((x, y, w, h, px.len()), (1, 1, 1, 1, 4));
        assert!(crop(&[0u8; 16], (2, 2)).is_none());
    }

    #[test]
    fn render_premultiplies_swaps_and_cuts_faint_alpha() {
        let mut img = RgbaImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgba([200, 100, 50, 128]));
        img.put_pixel(1, 0, image::Rgba([255, 255, 255, ALPHA_CUTOFF - 1]));
        let out = render_bgra(img, 2, 1);
        // 事前乗算（×128/255）してから B と R を入れ替える
        assert_eq!(&out[0..4], &[25, 50, 100, 128]);
        assert_eq!(&out[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn fit_keeps_flat_colors_and_corners() {
        // 一色の絵は、引き伸ばしても同じ色（丸めで 1 ずれない）
        let px: Vec<u8> = [10u8, 20, 30, 255].repeat(3 * 2);
        let out = fit(px, (3, 2), (5, 4));
        assert_eq!(out.len(), 5 * 4 * 4);
        assert!(out.chunks(4).all(|p| p == [10, 20, 30, 255]), "{out:?}");
        // 左が透明・右が不透明の 2 画素 → 左端は透明、右端は不透明、間は中間
        let px = vec![0, 0, 0, 0, 200, 200, 200, 255];
        let out = fit(px, (2, 1), (4, 1));
        assert_eq!(&out[0..4], &[0, 0, 0, 0]);
        assert_eq!(&out[12..16], &[200, 200, 200, 255]);
        assert!(out[7] > 0 && out[7] < 255);
        // 同じ大きさなら何もしない
        assert_eq!(fit(vec![1, 2, 3, 4], (1, 1), (1, 1)), vec![1, 2, 3, 4]);
    }

    #[test]
    fn stretch_keeps_the_bottom_row() {
        // 1×4 で一番下だけ不透明。伸ばしても一番下は不透明のまま（足元を固定）
        let mut px = vec![0u8; 16];
        px[12..16].copy_from_slice(&[9, 9, 9, 255]);
        let out = stretch_up(&px, (1, 4), 1.5);
        assert_eq!(out.len(), 16);
        assert_eq!(out[15], 255);
        assert_eq!(stretch_up(&px, (1, 4), 1.0), px);
    }
}
