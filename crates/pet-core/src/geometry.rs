//! 物理ピクセル座標の矩形と点。Windows の Per-Monitor DPI v2 では
//! 窓もディスプレイもこの座標系で扱う（副画面が主画面の左上にあると負の値になる）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// 右端・下端を含まない矩形（Win32 の RECT と同じ）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn from_xywh(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self::new(x, y, x + w, y + h)
    }

    pub const fn width(&self) -> i32 {
        self.right - self.left
    }

    pub const fn height(&self) -> i32 {
        self.bottom - self.top
    }

    pub const fn center(&self) -> Point {
        Point {
            x: self.left + self.width() / 2,
            y: self.top + self.height() / 2,
        }
    }

    pub const fn contains(&self, p: Point) -> bool {
        p.x >= self.left && p.x < self.right && p.y >= self.top && p.y < self.bottom
    }

    /// 重なりの面積（重ならなければ 0）。
    pub fn intersection_area(&self, other: &Rect) -> i64 {
        let w = self.right.min(other.right) - self.left.max(other.left);
        let h = self.bottom.min(other.bottom) - self.top.max(other.top);
        if w <= 0 || h <= 0 {
            0
        } else {
            i64::from(w) * i64::from(h)
        }
    }

    /// 点から矩形までの距離の二乗（内側なら 0）。
    pub fn distance_sq(&self, p: Point) -> i64 {
        let dx = i64::from((self.left - p.x).max(0).max(p.x - (self.right - 1)));
        let dy = i64::from((self.top - p.y).max(0).max(p.y - (self.bottom - 1)));
        dx * dx + dy * dy
    }

    pub const fn offset(&self, dx: i32, dy: i32) -> Self {
        Self::new(
            self.left + dx,
            self.top + dy,
            self.right + dx,
            self.bottom + dy,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersection_area_counts_only_overlap() {
        let a = Rect::from_xywh(0, 0, 100, 100);
        assert_eq!(
            a.intersection_area(&Rect::from_xywh(50, 50, 100, 100)),
            2500
        );
        assert_eq!(a.intersection_area(&Rect::from_xywh(100, 0, 10, 10)), 0);
        assert_eq!(
            a.intersection_area(&Rect::from_xywh(-383, -1440, 3440, 1440)),
            0
        );
    }

    #[test]
    fn distance_is_zero_inside_and_grows_outside() {
        let r = Rect::from_xywh(0, 0, 10, 10);
        assert_eq!(r.distance_sq(Point { x: 5, y: 5 }), 0);
        assert_eq!(r.distance_sq(Point { x: 12, y: 5 }), 9);
        assert_eq!(r.distance_sq(Point { x: -3, y: -4 }), 25);
    }

    #[test]
    fn contains_excludes_right_and_bottom_edges() {
        let r = Rect::from_xywh(0, 0, 10, 10);
        assert!(r.contains(Point { x: 9, y: 9 }));
        assert!(!r.contains(Point { x: 10, y: 0 }));
    }
}
