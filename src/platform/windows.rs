use std::{ffi::c_void, mem, path::Path, thread, time::Duration};

use anyhow::{bail, Context, Result};

use super::WindowManager;
use crate::model::{Monitor, Rect, Window, WindowKey};

type Bool = i32;
type Dword = u32;
type Handle = *mut c_void;
type Hwnd = *mut c_void;
type Hmonitor = *mut c_void;
type Lparam = isize;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct WinRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct MonitorInfo {
    cb_size: Dword,
    monitor: WinRect,
    work: WinRect,
    flags: Dword,
}

#[link(name = "user32")]
extern "system" {
    fn EnumWindows(callback: unsafe extern "system" fn(Hwnd, Lparam) -> Bool, data: Lparam)
        -> Bool;
    fn EnumDisplayMonitors(
        dc: Handle,
        clip: *const WinRect,
        callback: unsafe extern "system" fn(Hmonitor, Handle, *mut WinRect, Lparam) -> Bool,
        data: Lparam,
    ) -> Bool;
    fn IsWindowVisible(window: Hwnd) -> Bool;
    fn IsIconic(window: Hwnd) -> Bool;
    fn IsWindow(window: Hwnd) -> Bool;
    fn GetWindow(window: Hwnd, command: u32) -> Hwnd;
    fn GetWindowLongPtrW(window: Hwnd, index: i32) -> isize;
    fn GetWindowTextLengthW(window: Hwnd) -> i32;
    fn GetWindowTextW(window: Hwnd, buffer: *mut u16, maximum: i32) -> i32;
    fn GetWindowRect(window: Hwnd, rect: *mut WinRect) -> Bool;
    fn GetWindowThreadProcessId(window: Hwnd, process_id: *mut Dword) -> Dword;
    fn GetForegroundWindow() -> Hwnd;
    fn SetForegroundWindow(window: Hwnd) -> Bool;
    fn BringWindowToTop(window: Hwnd) -> Bool;
    fn ShowWindowAsync(window: Hwnd, command: i32) -> Bool;
    fn MonitorFromWindow(window: Hwnd, flags: Dword) -> Hmonitor;
    fn GetMonitorInfoW(monitor: Hmonitor, info: *mut MonitorInfo) -> Bool;
    fn SetWindowPos(
        window: Hwnd,
        insert_after: Hwnd,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> Bool;
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: Dword, inherit: Bool, process_id: Dword) -> Handle;
    fn QueryFullProcessImageNameW(
        process: Handle,
        flags: Dword,
        path: *mut u16,
        size: *mut Dword,
    ) -> Bool;
    fn CloseHandle(handle: Handle) -> Bool;
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmGetWindowAttribute(
        window: Hwnd,
        attribute: Dword,
        value: *mut c_void,
        size: Dword,
    ) -> i32;
}

const GW_OWNER: u32 = 4;
const GWL_EXSTYLE: i32 = -20;
const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
const DWMWA_CLOAKED: Dword = 14;
const MONITOR_DEFAULTTONEAREST: Dword = 2;
const PROCESS_QUERY_LIMITED_INFORMATION: Dword = 0x1000;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const SW_RESTORE: i32 = 9;

pub struct WindowsWindowManager;

impl WindowsWindowManager {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn window(&self, handle: Hwnd) -> Option<Window> {
        if !is_arrangeable(handle) {
            return None;
        }
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(handle, &mut pid) };
        if pid == 0 {
            return None;
        }
        let process = process_name(pid)?;
        let title = window_title(handle);
        if title.trim().is_empty() {
            return None;
        }
        let bounds = window_rect(handle)?;
        let monitor = unsafe { MonitorFromWindow(handle, MONITOR_DEFAULTTONEAREST) };
        if monitor.is_null() {
            return None;
        }
        Some(Window {
            key: WindowKey {
                pid,
                id: format!("{:x}", handle as usize),
            },
            process,
            title,
            bounds,
            monitor_id: monitor_id(monitor),
        })
    }
}

impl WindowManager for WindowsWindowManager {
    fn visible_windows(&self) -> Result<Vec<Window>> {
        let mut handles: Vec<Hwnd> = Vec::new();
        let ok = unsafe { EnumWindows(collect_window, &mut handles as *mut _ as Lparam) };
        if ok == 0 {
            bail!("Win32 failed to enumerate windows");
        }
        Ok(handles
            .into_iter()
            .filter_map(|handle| self.window(handle))
            .collect())
    }

    fn focused_window(&self) -> Result<Option<Window>> {
        let handle = unsafe { GetForegroundWindow() };
        Ok((!handle.is_null()).then(|| self.window(handle)).flatten())
    }

    fn focus_window(&self, window: &Window) -> Result<()> {
        let handle = window_handle(window)?;
        unsafe {
            ShowWindowAsync(handle, SW_RESTORE);
            BringWindowToTop(handle);
            SetForegroundWindow(handle);
        }
        for _ in 0..10 {
            if unsafe { GetForegroundWindow() } == handle {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
        bail!("Windows did not allow the window to become the active foreground window")
    }

    fn monitors(&self) -> Result<Vec<Monitor>> {
        let mut monitors = Vec::new();
        let ok = unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                collect_monitor,
                &mut monitors as *mut _ as Lparam,
            )
        };
        if ok == 0 {
            bail!("Win32 failed to enumerate monitors");
        }
        Ok(monitors)
    }

    fn set_bounds(&self, window: &Window, bounds: Rect) -> Result<()> {
        let handle = window_handle(window)?;
        let ok = unsafe {
            SetWindowPos(
                handle,
                std::ptr::null_mut(),
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        if ok == 0 {
            bail!(
                "SetWindowPos failed with OS error {}",
                std::io::Error::last_os_error()
            );
        }

        // A window may enforce a minimum size larger than its grid cell. Read
        // back the accepted size and keep it within its original monitor.
        let actual = window_rect(handle).context("window disappeared after SetWindowPos")?;
        let work_area = self
            .monitors()?
            .into_iter()
            .find(|monitor| monitor.id == window.monitor_id)
            .context("window monitor is no longer available")?
            .work_area;
        let (x, y) = work_area.clamp_position(bounds.x, bounds.y, actual.width, actual.height);
        if x != actual.x || y != actual.y {
            let ok = unsafe {
                SetWindowPos(
                    handle,
                    std::ptr::null_mut(),
                    x,
                    y,
                    actual.width,
                    actual.height,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
            };
            if ok == 0 {
                bail!(
                    "SetWindowPos failed while clamping with OS error {}",
                    std::io::Error::last_os_error()
                );
            }
        }
        Ok(())
    }
}

fn window_handle(window: &Window) -> Result<Hwnd> {
    let raw = usize::from_str_radix(&window.key.id, 16)
        .map_err(|_| anyhow::anyhow!("invalid Win32 window identifier"))?;
    let handle = raw as Hwnd;
    let mut pid = 0;
    if unsafe { IsWindow(handle) } == 0
        || unsafe { GetWindowThreadProcessId(handle, &mut pid) } == 0
        || pid != window.key.pid
    {
        bail!("window is no longer available");
    }
    Ok(handle)
}

unsafe extern "system" fn collect_window(window: Hwnd, data: Lparam) -> Bool {
    let handles = &mut *(data as *mut Vec<Hwnd>);
    handles.push(window);
    1
}

unsafe extern "system" fn collect_monitor(
    monitor: Hmonitor,
    _: Handle,
    _: *mut WinRect,
    data: Lparam,
) -> Bool {
    let monitors = &mut *(data as *mut Vec<Monitor>);
    let mut info = MonitorInfo {
        cb_size: mem::size_of::<MonitorInfo>() as u32,
        monitor: WinRect::default(),
        work: WinRect::default(),
        flags: 0,
    };
    if GetMonitorInfoW(monitor, &mut info) != 0 {
        monitors.push(Monitor {
            id: monitor_id(monitor),
            work_area: from_win_rect(info.work),
        });
    }
    1
}

fn is_arrangeable(window: Hwnd) -> bool {
    if unsafe { IsWindowVisible(window) } == 0 || unsafe { IsIconic(window) } != 0 {
        return false;
    }
    if !unsafe { GetWindow(window, GW_OWNER) }.is_null() {
        return false;
    }
    if unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) } & WS_EX_TOOLWINDOW != 0 {
        return false;
    }
    let mut cloaked = 0_u32;
    let result = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED,
            &mut cloaked as *mut _ as *mut c_void,
            mem::size_of_val(&cloaked) as u32,
        )
    };
    if result >= 0 && cloaked != 0 {
        return false;
    }
    window_rect(window).is_some_and(|rect| rect.width >= 80 && rect.height >= 60)
}

fn window_rect(window: Hwnd) -> Option<Rect> {
    let mut rect = WinRect::default();
    (unsafe { GetWindowRect(window, &mut rect) } != 0).then(|| from_win_rect(rect))
}

fn from_win_rect(rect: WinRect) -> Rect {
    Rect {
        x: rect.left,
        y: rect.top,
        width: rect.right - rect.left,
        height: rect.bottom - rect.top,
    }
}

fn window_title(window: Hwnd) -> String {
    let length = unsafe { GetWindowTextLengthW(window) };
    if length <= 0 {
        return String::new();
    }
    let mut buffer = vec![0_u16; length as usize + 1];
    let written =
        unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..written])
}

fn process_name(pid: u32) -> Option<String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut buffer = vec![0_u16; 32_768];
    let mut length = buffer.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) };
    unsafe { CloseHandle(process) };
    if ok == 0 {
        return None;
    }
    let path = String::from_utf16_lossy(&buffer[..length as usize]);
    let file = Path::new(&path).file_name()?.to_string_lossy();
    Some(file.strip_suffix(".exe").unwrap_or(&file).to_owned())
}

fn monitor_id(monitor: Hmonitor) -> String {
    format!("{:x}", monitor as usize)
}
