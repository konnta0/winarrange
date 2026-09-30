use std::{
    collections::HashMap,
    ffi::{c_char, c_int, c_long, c_void, CStr},
    process::Command,
    ptr,
};

use anyhow::{bail, Context, Result};

use super::WindowManager;
use crate::model::{Monitor, Rect, Window, WindowKey};

type CFTypeRef = *const c_void;
type CFArrayRef = *const c_void;
type CFDictionaryRef = *const c_void;
type CFStringRef = *const c_void;
type AXUIElementRef = *const c_void;
type AXValueRef = *const c_void;
type AXError = i32;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGSize {
    width: f64,
    height: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NSRect {
    origin: CGPoint,
    size: CGSize,
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(value: CFTypeRef);
    fn CFArrayGetCount(array: CFArrayRef) -> c_long;
    fn CFArrayGetValueAtIndex(array: CFArrayRef, index: c_long) -> *const c_void;
    fn CFDictionaryGetValue(dictionary: CFDictionaryRef, key: *const c_void) -> *const c_void;
    fn CFNumberGetValue(number: CFTypeRef, number_type: c_int, value: *mut c_void) -> bool;
    fn CFStringGetCString(
        string: CFStringRef,
        buffer: *mut c_char,
        size: c_long,
        encoding: u32,
    ) -> bool;
    fn CFStringCreateWithCString(
        allocator: CFTypeRef,
        string: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFBooleanGetValue(boolean: CFTypeRef) -> bool;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> AXError;
    fn AXValueCreate(value_type: c_int, value: *const c_void) -> AXValueRef;
    fn AXValueGetValue(value: AXValueRef, value_type: c_int, output: *mut c_void) -> bool;

    fn CGWindowListCopyWindowInfo(options: u32, relative_to_window: u32) -> CFArrayRef;
    fn CGRectMakeWithDictionaryRepresentation(
        dictionary: CFDictionaryRef,
        rect: *mut CGRect,
    ) -> bool;

    static kCGWindowNumber: CFStringRef;
    static kCGWindowOwnerPID: CFStringRef;
    static kCGWindowOwnerName: CFStringRef;
    static kCGWindowName: CFStringRef;
    static kCGWindowBounds: CFStringRef;
    static kCGWindowLayer: CFStringRef;
    static kCGWindowAlpha: CFStringRef;

}

#[link(name = "objc")]
#[allow(clashing_extern_declarations)]
extern "C" {
    fn objc_getClass(name: *const c_char) -> *mut c_void;
    fn sel_registerName(name: *const c_char) -> *mut c_void;
    #[link_name = "objc_msgSend"]
    fn msg_send_id(receiver: *mut c_void, selector: *mut c_void) -> *mut c_void;
    #[link_name = "objc_msgSend"]
    fn msg_send_id_i32(receiver: *mut c_void, selector: *mut c_void, value: i32) -> *mut c_void;
    #[link_name = "objc_msgSend"]
    fn msg_send_bool_usize(receiver: *mut c_void, selector: *mut c_void, value: usize) -> i8;
    #[link_name = "objc_msgSend"]
    fn msg_send_index(receiver: *mut c_void, selector: *mut c_void, index: usize) -> *mut c_void;
    #[link_name = "objc_msgSend"]
    fn msg_send_usize(receiver: *mut c_void, selector: *mut c_void) -> usize;
    #[link_name = "objc_msgSend"]
    fn msg_send_i32(receiver: *mut c_void, selector: *mut c_void) -> i32;
    #[link_name = "objc_msgSend"]
    fn msg_send_rect(receiver: *mut c_void, selector: *mut c_void) -> NSRect;
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

// Ensure AppKit is loaded before looking up NSScreen through the Objective-C runtime.
#[link(name = "AppKit", kind = "framework")]
extern "C" {}

const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const K_CF_NUMBER_SINT64_TYPE: c_int = 4;
const K_CF_NUMBER_DOUBLE_TYPE: c_int = 13;
const K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: u32 = 1;
const K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS: u32 = 16;
const K_CG_NULL_WINDOW_ID: u32 = 0;
const K_AX_VALUE_CGPOINT_TYPE: c_int = 1;
const K_AX_VALUE_CGSIZE_TYPE: c_int = 2;
const NS_APPLICATION_ACTIVATE_IGNORING_OTHER_APPS: usize = 1 << 1;

pub struct MacOsWindowManager;

impl MacOsWindowManager {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn ensure_accessibility(&self) -> Result<()> {
        if unsafe { AXIsProcessTrusted() } {
            Ok(())
        } else {
            bail!("Accessibility permission is required. Enable winarrange (or your terminal) in System Settings > Privacy & Security > Accessibility, then run the command again")
        }
    }

    fn cg_windows(&self) -> Result<Vec<CgWindow>> {
        let array = unsafe {
            CGWindowListCopyWindowInfo(
                K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY | K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS,
                K_CG_NULL_WINDOW_ID,
            )
        };
        if array.is_null() {
            bail!("CoreGraphics failed to enumerate windows");
        }
        let _array = CfGuard(array);
        let monitors = self.monitors()?;
        let mut result = Vec::new();
        let count = unsafe { CFArrayGetCount(array) };
        for index in 0..count {
            let dictionary = unsafe { CFArrayGetValueAtIndex(array, index) } as CFDictionaryRef;
            let layer = cf_i64(dict_get(dictionary, unsafe { kCGWindowLayer })).unwrap_or(-1);
            let alpha = cf_f64(dict_get(dictionary, unsafe { kCGWindowAlpha })).unwrap_or(1.0);
            if layer != 0 || alpha <= 0.0 {
                continue;
            }
            let Some(id) = cf_i64(dict_get(dictionary, unsafe { kCGWindowNumber })) else {
                continue;
            };
            let Some(pid) = cf_i64(dict_get(dictionary, unsafe { kCGWindowOwnerPID })) else {
                continue;
            };
            let process =
                cf_string(dict_get(dictionary, unsafe { kCGWindowOwnerName })).unwrap_or_default();
            let title =
                cf_string(dict_get(dictionary, unsafe { kCGWindowName })).unwrap_or_default();
            let bounds_dictionary =
                dict_get(dictionary, unsafe { kCGWindowBounds }) as CFDictionaryRef;
            let mut cg_rect = CGRect::default();
            if bounds_dictionary.is_null()
                || !unsafe {
                    CGRectMakeWithDictionaryRepresentation(bounds_dictionary, &mut cg_rect)
                }
            {
                continue;
            }
            let bounds = rect_from_cg(cg_rect);
            if bounds.width < 80 || bounds.height < 60 || process.is_empty() {
                continue;
            }
            let Some(monitor_id) = monitor_for(bounds, &monitors) else {
                continue;
            };
            result.push(CgWindow {
                id: id as u32,
                pid: pid as u32,
                process,
                title,
                bounds,
                monitor_id,
            });
        }
        Ok(result)
    }

    fn find_ax_window(&self, target: &Window) -> Result<AxGuard> {
        let application = unsafe { AXUIElementCreateApplication(target.key.pid as i32) };
        if application.is_null() {
            bail!("application is no longer running");
        }
        let _application = CfGuard(application);
        let windows =
            ax_copy(application, c"AXWindows").context("cannot read application windows")?;
        let _windows = CfGuard(windows);
        let count = unsafe { CFArrayGetCount(windows) };
        let mut best: Option<(i64, AXUIElementRef)> = None;
        for index in 0..count {
            let element = unsafe { CFArrayGetValueAtIndex(windows, index) } as AXUIElementRef;
            if !is_standard_ax_window(element) {
                continue;
            }
            let title = ax_string(element, c"AXTitle").unwrap_or_default();
            let bounds = ax_bounds(element).unwrap_or_default();
            let score = if title == target.title { 0 } else { 1_000_000 }
                + i64::from(
                    (bounds.x - target.bounds.x).abs() + (bounds.y - target.bounds.y).abs(),
                )
                + i64::from(
                    (bounds.width - target.bounds.width).abs()
                        + (bounds.height - target.bounds.height).abs(),
                );
            if best.is_none_or(|(best_score, _)| score < best_score) {
                best = Some((score, element));
            }
        }
        let (_, element) = best.context("window is no longer available")?;
        unsafe {
            CFRetain(element);
        }
        Ok(AxGuard(element))
    }
}

impl WindowManager for MacOsWindowManager {
    fn visible_windows(&self) -> Result<Vec<Window>> {
        self.ensure_accessibility()?;
        let windows = self.cg_windows()?;
        let mut by_pid: HashMap<u32, Vec<AXUIElementRef>> = HashMap::new();
        let mut applications = Vec::new();
        let mut arrays = Vec::new();
        for item in &windows {
            if by_pid.contains_key(&item.pid) {
                continue;
            }
            let application = unsafe { AXUIElementCreateApplication(item.pid as i32) };
            if application.is_null() {
                continue;
            }
            applications.push(CfGuard(application));
            let Some(array) = ax_copy(application, c"AXWindows") else {
                continue;
            };
            arrays.push(CfGuard(array));
            let count = unsafe { CFArrayGetCount(array) };
            let values = (0..count)
                .map(|i| unsafe { CFArrayGetValueAtIndex(array, i) } as AXUIElementRef)
                .collect();
            by_pid.insert(item.pid, values);
        }

        Ok(windows
            .into_iter()
            .filter(|item| {
                by_pid.get(&item.pid).is_some_and(|elements| {
                    elements.iter().any(|&element| {
                        is_standard_ax_window(element)
                            && !ax_bool(element, c"AXMinimized").unwrap_or(false)
                            && ax_bounds(element).is_some_and(|bounds| {
                                (bounds.x - item.bounds.x).abs() <= 8
                                    && (bounds.y - item.bounds.y).abs() <= 8
                                    && (bounds.width - item.bounds.width).abs() <= 8
                                    && (bounds.height - item.bounds.height).abs() <= 8
                            })
                    })
                })
            })
            .map(CgWindow::into_window)
            .collect())
    }

    fn focused_window(&self) -> Result<Option<Window>> {
        self.ensure_accessibility()?;
        let Some(pid) = frontmost_application_pid() else {
            return Ok(None);
        };
        let application = unsafe { AXUIElementCreateApplication(pid as i32) };
        if application.is_null() {
            return Ok(None);
        }
        let _application = CfGuard(application);
        let focused = ax_copy(application, c"AXFocusedWindow")
            .or_else(|| ax_copy(application, c"AXMainWindow"));
        let _focused = focused.map(CfGuard);
        let focused_details = focused
            .filter(|&element| is_standard_ax_window(element))
            .map(|element| {
                (
                    ax_string(element, c"AXTitle").unwrap_or_default(),
                    ax_bounds(element).unwrap_or_default(),
                )
            });

        let candidates: Vec<_> = self
            .cg_windows()?
            .into_iter()
            .filter(|window| window.pid == pid)
            .collect();
        let candidate = if let Some((title, bounds)) = focused_details {
            candidates.into_iter().min_by_key(|window| {
                let title_penalty = if window.title == title { 0 } else { 1_000_000 };
                title_penalty
                    + (window.bounds.x - bounds.x).abs()
                    + (window.bounds.y - bounds.y).abs()
            })
        } else {
            // CoreGraphics returns windows front-to-back, so the first normal
            // window is the best fallback when an app omits AXFocusedWindow.
            candidates.into_iter().next()
        };
        Ok(candidate.map(CgWindow::into_window))
    }

    fn focus_window(&self, window: &Window) -> Result<()> {
        self.ensure_accessibility()?;
        let element = self.find_ax_window(window)?;
        let application = unsafe { AXUIElementCreateApplication(window.key.pid as i32) };
        if application.is_null() {
            bail!("application is no longer running");
        }
        let _application = CfGuard(application);

        let focused_attribute = cf_string_create(c"AXFocusedWindow")?;
        let focus_error =
            unsafe { AXUIElementSetAttributeValue(application, focused_attribute.0, element.0) };
        let raise_action = cf_string_create(c"AXRaise")?;
        let raise_error = unsafe { AXUIElementPerformAction(element.0, raise_action.0) };
        if focus_error != 0 && raise_error != 0 {
            bail!(
                "Accessibility API could not focus the window (focus error {focus_error}, raise error {raise_error})"
            );
        }

        activate_application(window.key.pid)
    }

    fn monitors(&self) -> Result<Vec<Monitor>> {
        unsafe {
            let pool = objc_autoreleasePoolPush();
            let class = objc_getClass(c"NSScreen".as_ptr());
            if class.is_null() {
                objc_autoreleasePoolPop(pool);
                bail!("AppKit NSScreen is unavailable");
            }
            let screens = msg_send_id(class, sel_registerName(c"screens".as_ptr()));
            let count = msg_send_usize(screens, sel_registerName(c"count".as_ptr()));
            let main = msg_send_id(class, sel_registerName(c"mainScreen".as_ptr()));
            let main_frame = msg_send_rect(main, sel_registerName(c"frame".as_ptr()));
            let origin_y = main_frame.origin.y + main_frame.size.height;
            let mut result = Vec::with_capacity(count);
            for index in 0..count {
                let screen =
                    msg_send_index(screens, sel_registerName(c"objectAtIndex:".as_ptr()), index);
                let visible = msg_send_rect(screen, sel_registerName(c"visibleFrame".as_ptr()));
                result.push(Monitor {
                    id: (index + 1).to_string(),
                    work_area: Rect {
                        x: visible.origin.x.round() as i32,
                        y: (origin_y - visible.origin.y - visible.size.height).round() as i32,
                        width: visible.size.width.round() as i32,
                        height: visible.size.height.round() as i32,
                    },
                });
            }
            objc_autoreleasePoolPop(pool);
            Ok(result)
        }
    }

    fn set_bounds(&self, window: &Window, bounds: Rect) -> Result<()> {
        self.ensure_accessibility()?;
        let element = self.find_ax_window(window)?;
        let size = CGSize {
            width: bounds.width as f64,
            height: bounds.height as f64,
        };
        let size_value =
            unsafe { AXValueCreate(K_AX_VALUE_CGSIZE_TYPE, &size as *const _ as *const c_void) };
        if size_value.is_null() {
            bail!("failed to create Accessibility geometry values");
        }
        let _size = CfGuard(size_value);
        let size_attribute = cf_string_create(c"AXSize")?;
        let size_error =
            unsafe { AXUIElementSetAttributeValue(element.0, size_attribute.0, size_value) };
        if size_error != 0 {
            bail!("Accessibility API rejected window size (error {size_error})");
        }

        // Applications may enforce a minimum size. Read the accepted size and
        // clamp the subsequent position so a large window cannot be placed
        // beyond the right or bottom edge of its monitor.
        let actual = ax_bounds(element.0).unwrap_or(bounds);
        let work_area = self
            .monitors()?
            .into_iter()
            .find(|monitor| monitor.id == window.monitor_id)
            .context("window monitor is no longer available")?
            .work_area;
        let (x, y) = work_area.clamp_position(bounds.x, bounds.y, actual.width, actual.height);
        let position = CGPoint {
            x: x as f64,
            y: y as f64,
        };
        let position_value = unsafe {
            AXValueCreate(
                K_AX_VALUE_CGPOINT_TYPE,
                &position as *const _ as *const c_void,
            )
        };
        if position_value.is_null() {
            bail!("failed to create an Accessibility position value");
        }
        let _position = CfGuard(position_value);
        let position_attribute = cf_string_create(c"AXPosition")?;
        let position_error = unsafe {
            AXUIElementSetAttributeValue(element.0, position_attribute.0, position_value)
        };
        if position_error != 0 {
            bail!("Accessibility API rejected window position (error {position_error})");
        }
        Ok(())
    }
}

struct CgWindow {
    id: u32,
    pid: u32,
    process: String,
    title: String,
    bounds: Rect,
    monitor_id: String,
}
impl CgWindow {
    fn into_window(self) -> Window {
        Window {
            key: WindowKey {
                pid: self.pid,
                id: self.id.to_string(),
            },
            process: self.process,
            title: self.title,
            bounds: self.bounds,
            monitor_id: self.monitor_id,
        }
    }
}

struct CfGuard(CFTypeRef);
impl Drop for CfGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}
struct AxGuard(AXUIElementRef);
impl Drop for AxGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRetain(value: CFTypeRef) -> CFTypeRef;
}

fn dict_get(dictionary: CFDictionaryRef, key: CFStringRef) -> CFTypeRef {
    if dictionary.is_null() {
        ptr::null()
    } else {
        unsafe { CFDictionaryGetValue(dictionary, key) }
    }
}
fn cf_i64(value: CFTypeRef) -> Option<i64> {
    let mut output = 0_i64;
    (!value.is_null()
        && unsafe {
            CFNumberGetValue(
                value,
                K_CF_NUMBER_SINT64_TYPE,
                &mut output as *mut _ as *mut c_void,
            )
        })
    .then_some(output)
}
fn cf_f64(value: CFTypeRef) -> Option<f64> {
    let mut output = 0.0;
    (!value.is_null()
        && unsafe {
            CFNumberGetValue(
                value,
                K_CF_NUMBER_DOUBLE_TYPE,
                &mut output as *mut _ as *mut c_void,
            )
        })
    .then_some(output)
}
fn cf_string(value: CFTypeRef) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let mut buffer = vec![0_i8; 4096];
    if !unsafe {
        CFStringGetCString(
            value,
            buffer.as_mut_ptr(),
            buffer.len() as c_long,
            K_CF_STRING_ENCODING_UTF8,
        )
    } {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned(),
    )
}
fn cf_string_create(value: &CStr) -> Result<CfGuard> {
    let string = unsafe {
        CFStringCreateWithCString(ptr::null(), value.as_ptr(), K_CF_STRING_ENCODING_UTF8)
    };
    if string.is_null() {
        bail!("failed to create a CoreFoundation string");
    }
    Ok(CfGuard(string))
}
fn ax_copy(element: AXUIElementRef, attribute: &CStr) -> Option<CFTypeRef> {
    let attribute = cf_string_create(attribute).ok()?;
    let mut value = ptr::null();
    (unsafe { AXUIElementCopyAttributeValue(element, attribute.0, &mut value) } == 0
        && !value.is_null())
    .then_some(value)
}
fn ax_string(element: AXUIElementRef, attribute: &CStr) -> Option<String> {
    let value = ax_copy(element, attribute)?;
    let _value = CfGuard(value);
    cf_string(value)
}
fn ax_bool(element: AXUIElementRef, attribute: &CStr) -> Option<bool> {
    let value = ax_copy(element, attribute)?;
    let _value = CfGuard(value);
    Some(unsafe { CFBooleanGetValue(value) })
}
fn is_standard_ax_window(element: AXUIElementRef) -> bool {
    ax_string(element, c"AXSubrole").is_some_and(|subrole| subrole == "AXStandardWindow")
}
fn ax_bounds(element: AXUIElementRef) -> Option<Rect> {
    let position_value = ax_copy(element, c"AXPosition")?;
    let _position_value = CfGuard(position_value);
    let size_value = ax_copy(element, c"AXSize")?;
    let _size_value = CfGuard(size_value);
    let mut position = CGPoint::default();
    let mut size = CGSize::default();
    if !unsafe {
        AXValueGetValue(
            position_value,
            K_AX_VALUE_CGPOINT_TYPE,
            &mut position as *mut _ as *mut c_void,
        )
    } || !unsafe {
        AXValueGetValue(
            size_value,
            K_AX_VALUE_CGSIZE_TYPE,
            &mut size as *mut _ as *mut c_void,
        )
    } {
        return None;
    }
    Some(Rect {
        x: position.x.round() as i32,
        y: position.y.round() as i32,
        width: size.width.round() as i32,
        height: size.height.round() as i32,
    })
}
fn rect_from_cg(rect: CGRect) -> Rect {
    Rect {
        x: rect.origin.x.round() as i32,
        y: rect.origin.y.round() as i32,
        width: rect.size.width.round() as i32,
        height: rect.size.height.round() as i32,
    }
}
fn monitor_for(bounds: Rect, monitors: &[Monitor]) -> Option<String> {
    monitors
        .iter()
        .max_by_key(|monitor| bounds.intersection_area(monitor.work_area))
        .map(|monitor| monitor.id.clone())
}

fn frontmost_application_pid() -> Option<u32> {
    unsafe {
        let pool = objc_autoreleasePoolPush();
        let class = objc_getClass(c"NSWorkspace".as_ptr());
        if class.is_null() {
            objc_autoreleasePoolPop(pool);
            return None;
        }
        let workspace = msg_send_id(class, sel_registerName(c"sharedWorkspace".as_ptr()));
        let application = msg_send_id(
            workspace,
            sel_registerName(c"frontmostApplication".as_ptr()),
        );
        let pid = if application.is_null() {
            0
        } else {
            msg_send_i32(application, sel_registerName(c"processIdentifier".as_ptr()))
        };
        objc_autoreleasePoolPop(pool);
        (pid > 0).then_some(pid as u32)
    }
}

fn activate_application(pid: u32) -> Result<()> {
    if frontmost_application_pid() == Some(pid) {
        return Ok(());
    }

    let _activated = unsafe {
        let pool = objc_autoreleasePoolPush();
        let class = objc_getClass(c"NSRunningApplication".as_ptr());
        if class.is_null() {
            objc_autoreleasePoolPop(pool);
            bail!("AppKit NSRunningApplication is unavailable");
        }
        let application = msg_send_id_i32(
            class,
            sel_registerName(c"runningApplicationWithProcessIdentifier:".as_ptr()),
            pid as i32,
        );
        if application.is_null() {
            objc_autoreleasePoolPop(pool);
            bail!("application with pid {pid} is no longer running");
        }
        let activated = msg_send_bool_usize(
            application,
            sel_registerName(c"activateWithOptions:".as_ptr()),
            NS_APPLICATION_ACTIVATE_IGNORING_OTHER_APPS,
        ) != 0;
        objc_autoreleasePoolPop(pool);
        activated
    };
    // AppKit can report success without actually making the process active.
    // Always follow it with the System Events operation known to update the
    // process-level active state. Its result also avoids NSWorkspace's
    // run-loop-scoped cache when verifying the new frontmost PID.
    let script = format!(
        "tell application \"System Events\"\nset frontmost of first application process whose unix id is {pid} to true\nreturn unix id of first application process whose frontmost is true\nend tell"
    );
    let output = Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .output()
        .context("failed to run the macOS application activation fallback")?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        bail!(
            "macOS application activation fallback failed: {}",
            message.trim()
        );
    }
    let frontmost_pid = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .context("macOS application activation returned an invalid frontmost pid")?;
    if frontmost_pid != pid {
        bail!("application pid {pid} did not become active (frontmost pid is {frontmost_pid})");
    }
    Ok(())
}
