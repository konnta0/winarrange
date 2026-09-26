use super::WindowManager;
use crate::model::{Monitor, Rect, Window};
use anyhow::{bail, Result};

pub struct UnsupportedWindowManager;

impl WindowManager for UnsupportedWindowManager {
    fn visible_windows(&self) -> Result<Vec<Window>> {
        bail!("winarrange supports only macOS and Windows")
    }
    fn focused_window(&self) -> Result<Option<Window>> {
        bail!("winarrange supports only macOS and Windows")
    }
    fn monitors(&self) -> Result<Vec<Monitor>> {
        bail!("winarrange supports only macOS and Windows")
    }
    fn set_bounds(&self, _: &Window, _: Rect) -> Result<()> {
        bail!("winarrange supports only macOS and Windows")
    }
}
