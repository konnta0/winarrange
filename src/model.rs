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
