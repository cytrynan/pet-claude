//! ペット窓の置き場所: どのディスプレイに属するか・作業領域へのクランプ・位置の保存と復元。
//!
//! 位置は「足元」（窓の下端中央）で持つ。表示倍率が変わると窓の大きさが変わるが、
//! 足元を固定すればキャラクターが立っている場所は動かない。

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::geometry::{Point, Rect};

/// 物理ピクセルでのディスプレイ 1 枚。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    /// ディスプレイ全体。
    pub bounds: Rect,
    /// タスクバーを除いた作業領域。
    pub work: Rect,
    /// 96 = 100%。
    pub dpi: u32,
    pub primary: bool,
}

impl Monitor {
    /// 保存位置の鍵。ディスプレイ ID は再起動で変わるので物理的な bounds で識別する。
    pub fn key(&self) -> String {
        let b = self.bounds;
        format!("{},{},{}x{}", b.left, b.top, b.width(), b.height())
    }
}

/// 窓が最も多く重なるディスプレイ。どれとも重ならなければ中心に最も近いもの。
pub fn monitor_for_rect<'a>(monitors: &'a [Monitor], window: &Rect) -> Option<&'a Monitor> {
    let best_overlap = monitors
        .iter()
        .map(|m| (m, m.bounds.intersection_area(window)))
        .filter(|(_, a)| *a > 0)
        .max_by_key(|(_, a)| *a)
        .map(|(m, _)| m);
    best_overlap.or_else(|| monitor_nearest_point(monitors, window.center()))
}

/// 点を含むディスプレイ。無ければ最も近いもの。
pub fn monitor_nearest_point(monitors: &[Monitor], p: Point) -> Option<&Monitor> {
    monitors.iter().min_by_key(|m| m.bounds.distance_sq(p))
}

/// 窓を作業領域の内側へ押し戻す。窓が作業領域より大きい軸は左上に揃える。
pub fn clamp_into(window: &Rect, work: &Rect) -> Rect {
    let w = window.width();
    let h = window.height();
    let x = if w >= work.width() {
        work.left
    } else {
        window.left.clamp(work.left, work.right - w)
    };
    let y = if h >= work.height() {
        work.top
    } else {
        window.top.clamp(work.top, work.bottom - h)
    };
    Rect::from_xywh(x, y, w, h)
}

/// 足元（下端中央）と窓の大きさから窓の矩形を作る。
pub const fn rect_from_anchor(anchor: Point, w: i32, h: i32) -> Rect {
    Rect::from_xywh(anchor.x - w / 2, anchor.y - h, w, h)
}

/// 窓の矩形から足元を取る。
pub const fn anchor_of(window: &Rect) -> Point {
    Point {
        x: window.left + window.width() / 2,
        y: window.bottom,
    }
}

/// 初期位置: 主ディスプレイの作業領域の右下から、窓の右端・下端が `inset` だけ内側に来る足元。
pub const fn default_anchor(monitor: &Monitor, inset: i32, window_w: i32) -> Point {
    Point {
        x: monitor.work.right - inset - (window_w - window_w / 2),
        y: monitor.work.bottom - inset,
    }
}

const MAX_SAVED_MONITORS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedAnchor {
    pub monitor: String,
    pub anchor: Point,
}

/// 利用者がドラッグで置いた位置の記録。
///
/// `home` は最後に置いたディスプレイと位置。ディスプレイが外れている間は別の画面に
/// 退避するが、`home` は書き換えない（付け直したら戻れるように）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionStore {
    pub home: Option<SavedAnchor>,
    /// ディスプレイごとの最後の位置。新しい順、最大 8 件。
    pub per_monitor: VecDeque<SavedAnchor>,
}

impl PositionStore {
    /// 利用者が置いた位置を記録する。
    pub fn remember(&mut self, monitor: &Monitor, anchor: Point) {
        let saved = SavedAnchor {
            monitor: monitor.key(),
            anchor,
        };
        self.per_monitor.retain(|s| s.monitor != saved.monitor);
        self.per_monitor.push_front(saved.clone());
        self.per_monitor.truncate(MAX_SAVED_MONITORS);
        self.home = Some(saved);
    }

    fn saved_for(&self, key: &str) -> Option<Point> {
        self.per_monitor
            .iter()
            .find(|s| s.monitor == key)
            .map(|s| s.anchor)
    }

    /// いまのディスプレイ構成で窓を置く矩形を決める（作業領域にクランプ済み）。
    ///
    /// 1. `home` のディスプレイがあればそこ
    /// 2. 無ければ主ディスプレイの保存位置
    /// 3. それも無ければ主ディスプレイの右下
    ///
    /// `size_for` はディスプレイの DPI から窓の大きさを返す。
    pub fn resolve(
        &self,
        monitors: &[Monitor],
        size_for: impl Fn(&Monitor) -> (i32, i32),
        inset: i32,
    ) -> Option<(Rect, Placement)> {
        let primary = monitors
            .iter()
            .find(|m| m.primary)
            .or_else(|| monitors.first())?;
        let home = self.home.as_ref().and_then(|h| {
            monitors
                .iter()
                .find(|m| m.key() == h.monitor)
                .map(|m| (m, h.anchor))
        });
        let (monitor, anchor, how) = match (home, self.saved_for(&primary.key())) {
            (Some((m, a)), _) => (m, a, Placement::Home),
            (None, Some(a)) => (primary, a, Placement::PrimarySaved),
            (None, None) => {
                let w = size_for(primary).0;
                (
                    primary,
                    default_anchor(primary, inset, w),
                    Placement::Default,
                )
            }
        };
        let (w, h) = size_for(monitor);
        Some((
            clamp_into(&rect_from_anchor(anchor, w, h), &monitor.work),
            how,
        ))
    }
}

/// `PositionStore::resolve` がどの規則で位置を決めたか（ログと検証用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Home,
    PrimarySaved,
    Default,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// K の実環境（2026-10-21 `pet-app --probe` で実測）: ノート 2880x1800 @200%（左にタスクバー）と、
    /// その上に 3440x1440 @100%（x が負側にはみ出す）。
    fn laptop() -> Monitor {
        Monitor {
            bounds: Rect::from_xywh(0, 0, 2880, 1800),
            work: Rect::new(96, 0, 2880, 1800),
            dpi: 192,
            primary: true,
        }
    }

    fn ultrawide() -> Monitor {
        Monitor {
            bounds: Rect::from_xywh(-383, -1440, 3440, 1440),
            work: Rect::new(-335, -1440, 3057, 0),
            dpi: 96,
            primary: false,
        }
    }

    fn size(m: &Monitor) -> (i32, i32) {
        let s = m.dpi as i32;
        (115 * s / 96, 125 * s / 96)
    }

    #[test]
    fn clamp_pushes_window_back_inside_each_edge() {
        let work = laptop().work;
        let r = clamp_into(&Rect::from_xywh(-50, -50, 100, 100), &work);
        assert_eq!((r.left, r.top), (96, 0));
        let r = clamp_into(&Rect::from_xywh(2900, 1900, 100, 100), &work);
        assert_eq!((r.right, r.bottom), (2880, 1800));
        let inside = Rect::from_xywh(500, 500, 100, 100);
        assert_eq!(clamp_into(&inside, &work), inside);
    }

    #[test]
    fn clamp_aligns_top_left_when_window_is_larger_than_work_area() {
        let work = Rect::from_xywh(0, 0, 50, 50);
        let r = clamp_into(&Rect::from_xywh(10, 10, 80, 80), &work);
        assert_eq!((r.left, r.top, r.width(), r.height()), (0, 0, 80, 80));
    }

    #[test]
    fn clamp_works_in_negative_coordinates() {
        let m = ultrawide();
        let r = clamp_into(&Rect::from_xywh(-1000, -2000, 115, 125), &m.work);
        assert_eq!((r.left, r.top), (-335, -1440));
    }

    #[test]
    fn monitor_for_rect_prefers_largest_overlap() {
        let ms = [laptop(), ultrawide()];
        let straddling = Rect::from_xywh(100, -20, 100, 130);
        assert!(monitor_for_rect(&ms, &straddling).unwrap().primary);
        let mostly_up = Rect::from_xywh(100, -120, 100, 130);
        assert!(!monitor_for_rect(&ms, &mostly_up).unwrap().primary);
    }

    #[test]
    fn monitor_for_rect_falls_back_to_nearest_when_offscreen() {
        let ms = [laptop(), ultrawide()];
        let far_below = Rect::from_xywh(1000, 3000, 100, 100);
        assert!(monitor_for_rect(&ms, &far_below).unwrap().primary);
        let far_up_left = Rect::from_xywh(-3000, -3000, 100, 100);
        assert!(!monitor_for_rect(&ms, &far_up_left).unwrap().primary);
    }

    #[test]
    fn anchor_round_trips_through_rect() {
        let a = Point { x: 300, y: 800 };
        let r = rect_from_anchor(a, 115, 125);
        assert_eq!(anchor_of(&r), a);
    }

    #[test]
    fn resolve_defaults_to_bottom_right_of_primary() {
        let store = PositionStore::default();
        let ms = [ultrawide(), laptop()];
        let (r, how) = store.resolve(&ms, size, 24).unwrap();
        assert_eq!(how, Placement::Default);
        let w = laptop().work;
        assert_eq!((r.right, r.bottom), (w.right - 24, w.bottom - 24));
    }

    #[test]
    fn resolve_returns_to_home_monitor_after_it_is_reattached() {
        let mut store = PositionStore::default();
        store.remember(&ultrawide(), Point { x: 1000, y: -100 });
        // 副画面を外す → 主画面に退避（home は消えない）
        let (r, how) = store.resolve(&[laptop()], size, 24).unwrap();
        assert_eq!(how, Placement::Default);
        assert!(laptop().work.contains(r.center()));
        assert!(store.home.is_some());
        // 付け直す → 元の位置に戻る
        let (r, how) = store.resolve(&[laptop(), ultrawide()], size, 24).unwrap();
        assert_eq!(how, Placement::Home);
        assert_eq!(anchor_of(&r), Point { x: 1000, y: -100 });
    }

    #[test]
    fn resolve_clamps_saved_position_that_now_falls_outside() {
        let mut store = PositionStore::default();
        store.remember(&laptop(), Point { x: 2870, y: 1800 });
        let (r, _) = store.resolve(&[laptop()], size, 24).unwrap();
        assert!(r.right <= 2880 && r.bottom <= 1800);
    }

    #[test]
    fn remember_keeps_at_most_eight_monitors_newest_first() {
        let mut store = PositionStore::default();
        for i in 0..10 {
            let m = Monitor {
                bounds: Rect::from_xywh(i * 100, 0, 100, 100),
                work: Rect::from_xywh(i * 100, 0, 100, 100),
                dpi: 96,
                primary: false,
            };
            store.remember(&m, Point { x: i, y: 0 });
        }
        assert_eq!(store.per_monitor.len(), 8);
        assert_eq!(store.per_monitor[0].anchor.x, 9);
    }

    #[test]
    fn resolve_without_monitors_is_none() {
        assert!(PositionStore::default().resolve(&[], size, 24).is_none());
    }

    #[test]
    fn store_round_trips_through_json() {
        let mut store = PositionStore::default();
        store.remember(&ultrawide(), Point { x: -10, y: -20 });
        let json = serde_json::to_string(&store).unwrap();
        assert_eq!(serde_json::from_str::<PositionStore>(&json).unwrap(), store);
    }
}
