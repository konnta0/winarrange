use anyhow::Result;

use crate::model::{Monitor, Rect, Window};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(target_os = "windows")]
mod windows;

pub trait WindowManager {
    fn visible_windows(&self) -> Result<Vec<Window>>;
    fn focused_window(&self) -> Result<Option<Window>>;
    fn monitors(&self) -> Result<Vec<Monitor>>;
    fn set_bounds(&self, window: &Window, bounds: Rect) -> Result<()>;
}

pub fn system_window_manager() -> Result<Box<dyn WindowManager>> {
    #[cfg(target_os = "macos")]
    return Ok(Box::new(macos::MacOsWindowManager::new()?));
    #[cfg(target_os = "windows")]
    return Ok(Box::new(windows::WindowsWindowManager::new()?));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return Ok(Box::new(unsupported::UnsupportedWindowManager));
}
