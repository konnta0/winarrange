#[cfg(any(target_os = "macos", target_os = "windows"))]
mod autostart;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod supported;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use supported::command;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn command(_: Option<crate::cli::DaemonCommand>) -> anyhow::Result<()> {
    anyhow::bail!("the hotkey daemon is only supported on macOS and Windows")
}
