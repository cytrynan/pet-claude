//! `pet_core::voice::looks_muted`（ミュートのマイクの形）の単体テスト。
//! 2026-10-22 K の「毎回聞き取れない」の原因（既定のマイクがミュート・最大振幅 1）から足した。

use pet_core::voice::{MUTED_PEAK_MAX, looks_muted};

#[test]
fn empty_is_not_muted() {
    assert!(!looks_muted(&[]));
}

#[test]
fn exact_zeros_are_muted() {
    assert!(looks_muted(&[0; 16000]));
}

#[test]
fn near_silence_like_a_muted_endpoint_is_muted() {
    // K の PC で測った形: -1〜1 の揺れだけ
    let s: Vec<i16> = (0..96_000).map(|i| (i % 3) as i16 - 1).collect();
    assert!(looks_muted(&s));
}

#[test]
fn boundary_is_inclusive_and_symmetric() {
    assert!(looks_muted(&[MUTED_PEAK_MAX, -MUTED_PEAK_MAX]));
    assert!(!looks_muted(&[0, MUTED_PEAK_MAX + 1]));
    assert!(!looks_muted(&[0, -MUTED_PEAK_MAX - 1]));
}

#[test]
fn i16_min_does_not_overflow() {
    assert!(!looks_muted(&[i16::MIN]));
}

#[test]
fn quiet_room_noise_is_not_muted() {
    // 生きているマイクの静かな部屋（±10 前後）
    let s: Vec<i16> = (0..16000).map(|i| (i * 37 % 21) as i16 - 10).collect();
    assert!(!looks_muted(&s));
}
