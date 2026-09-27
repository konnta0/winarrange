use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn area(self) -> i64 {
        i64::from(self.width.max(0)) * i64::from(self.height.max(0))
    }

    pub fn intersection_area(self, other: Self) -> i64 {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        i64::from((right - left).max(0)) * i64::from((bottom - top).max(0))
    }

    /// Keep the top-left corner of an object inside this rectangle, accounting
    /// for the object's actual size. Oversized objects are anchored at our
    /// top-left corner.
    pub fn clamp_position(self, x: i32, y: i32, width: i32, height: i32) -> (i32, i32) {
        let max_x = self
            .x
            .saturating_add(self.width)
            .saturating_sub(width.max(0))
            .max(self.x);
        let max_y = self
            .y
            .saturating_add(self.height)
            .saturating_sub(height.max(0))
            .max(self.y);
        (x.clamp(self.x, max_x), y.clamp(self.y, max_y))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Monitor {
    pub id: String,
    pub work_area: Rect,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowKey {
    pub pid: u32,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub key: WindowKey,
    pub process: String,
    pub title: String,
    pub bounds: Rect,
    pub monitor_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_windows_to_the_work_area() {
        let work_area = Rect {
            x: 2560,
            y: -480,
            width: 1080,
            height: 1600,
        };

        assert_eq!(
            work_area.clamp_position(3100, -480, 1291, 1187),
            (2560, -480)
        );
        assert_eq!(work_area.clamp_position(3000, 800, 600, 400), (3000, 720));
    }
}
