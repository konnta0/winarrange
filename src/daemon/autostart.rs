#[cfg(target_os = "macos")]
use std::{fs, path::PathBuf};

use anyhow::{Context, Result};

#[cfg(target_os = "macos")]
pub fn install() -> Result<()> {
    let executable = std::env::current_exe()
        .context("failed to locate winarrange executable")?
        .canonicalize()
        .context("failed to resolve winarrange executable path")?;
    let path = launch_agent_path()?;
    let parent = path.parent().context("invalid LaunchAgent path")?;
    fs::create_dir_all(parent)?;
    let executable = xml_escape(&executable.to_string_lossy());
    let contents = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>io.github.konnta0.winarrange</string>
  <key>ProgramArguments</key>
  <array>
    <string>{executable}</string>
    <string>daemon</string>
    <string>run</string>
    <string>--background</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#
    );
    fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn uninstall() -> Result<()> {
    let path = launch_agent_path()?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

#[cfg(target_os = "macos")]
fn launch_agent_path() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join("Library/LaunchAgents/io.github.konnta0.winarrange.plist"))
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "windows")]
pub fn install() -> Result<()> {
    let executable = std::env::current_exe()
        .context("failed to locate winarrange executable")?
        .canonicalize()
        .context("failed to resolve winarrange executable path")?;
    // `daemon start` creates the long-lived child with CREATE_NO_WINDOW. The
    // registry-launched process may flash briefly, but the daemon itself does
    // not keep a console window open for the login session.
    let command = format!("\"{}\" daemon start", executable.to_string_lossy());
    set_run_value(Some(&command))
}

#[cfg(target_os = "windows")]
pub fn uninstall() -> Result<()> {
    set_run_value(None)
}

#[cfg(target_os = "windows")]
fn set_run_value(value: Option<&str>) -> Result<()> {
    use std::{ffi::c_void, ptr};

    type Hkey = *mut c_void;
    const HKEY_CURRENT_USER: Hkey = 0x8000_0001_usize as Hkey;
    const KEY_SET_VALUE: u32 = 0x0002;
    const REG_SZ: u32 = 1;
    const ERROR_FILE_NOT_FOUND: i32 = 2;

    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(
            root: Hkey,
            subkey: *const u16,
            options: u32,
            access: u32,
            key: *mut Hkey,
        ) -> i32;
        fn RegSetValueExW(
            key: Hkey,
            name: *const u16,
            reserved: u32,
            value_type: u32,
            data: *const u8,
            size: u32,
        ) -> i32;
        fn RegDeleteValueW(key: Hkey, name: *const u16) -> i32;
        fn RegCloseKey(key: Hkey) -> i32;
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    let subkey = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let name = wide("winarrange");
    let mut key = ptr::null_mut();
    let result = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut key,
        )
    };
    if result != 0 {
        anyhow::bail!("failed to open current-user startup registry key (error {result})");
    }
    let result = if let Some(value) = value {
        let value = wide(value);
        unsafe {
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * std::mem::size_of::<u16>()) as u32,
            )
        }
    } else {
        unsafe { RegDeleteValueW(key, name.as_ptr()) }
    };
    unsafe { RegCloseKey(key) };
    if result != 0 && !(value.is_none() && result == ERROR_FILE_NOT_FOUND) {
        anyhow::bail!("failed to update current-user startup registry value (error {result})");
    }
    Ok(())
}
