//! What the screen looks like to the SteamVR headset window.
//!
//! SteamVR fixes the window size at startup and centres the window on the
//! primary monitor itself. vrserver is per-monitor DPI aware, so the sizes
//! read here are in physical pixels, the same unit the window size is in.
//!
//! The size is not changed while SteamVR runs: resizing the headset window
//! made vrcompositor crash inside the graphics driver.

use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SystemParametersInfoW, SM_CXSCREEN, SM_CYSCREEN, SPI_GETWORKAREA,
};

use crate::logic::WindowRect;

/// The primary monitor's work area (without the taskbar).
pub fn primary_work_area() -> Option<WindowRect> {
    let mut area = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: SPI_GETWORKAREA writes one RECT to the buffer it is given.
    let ok =
        unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut area as *mut RECT).cast(), 0) };
    (ok != 0).then_some(WindowRect {
        x: area.left,
        y: area.top,
        width: area.right - area.left,
        height: area.bottom - area.top,
    })
}

/// The whole primary monitor (taskbar included), on whose centre SteamVR
/// puts the headset window. The primary monitor starts at the origin.
pub fn primary_monitor() -> Option<WindowRect> {
    // SAFETY: plain queries without arguments to check.
    let (width, height) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    (width > 0 && height > 0).then_some(WindowRect {
        x: 0,
        y: 0,
        width,
        height,
    })
}
