//! SteamVR driver `mousehmd`: a virtual HMD whose orientation follows the mouse.
//!
//! The OpenVR driver interfaces are C++ classes, so they live in `cpp/shim.cpp`.
//! This crate owns the input polling and the orientation state, and exports the
//! DLL entry point SteamVR looks for.
//!
//! The driver only exists on Windows. `logic` has no platform dependencies, so
//! its tests also run elsewhere.

#![deny(unsafe_op_in_unsafe_fn)]
// Off Windows only the tests use `logic`.
#![cfg_attr(not(windows), allow(dead_code))]

mod logic;

#[cfg(windows)]
mod input;

#[cfg(windows)]
mod window;

#[cfg(windows)]
mod ffi {
    use std::ffi::{c_char, c_int, c_void};

    use crate::logic::{self, Config, DisplayConfig, WindowRect};
    use crate::{input, window};

    extern "C" {
        fn mousehmd_cpp_factory(
            interface_name: *const c_char,
            return_code: *mut c_int,
        ) -> *mut c_void;
    }

    /// Entry point SteamVR resolves in `driver_mousehmd.dll`.
    ///
    /// # Safety
    /// Called by vrserver with a valid interface name and return-code pointer.
    #[no_mangle]
    pub unsafe extern "C" fn HmdDriverFactory(
        interface_name: *const c_char,
        return_code: *mut c_int,
    ) -> *mut c_void {
        // SAFETY: both pointers come straight from vrserver and are passed through unchanged.
        unsafe { mousehmd_cpp_factory(interface_name, return_code) }
    }

    /// Starts the input thread. Returns false if it could not be started, in
    /// which case the driver must not report itself as initialized.
    ///
    /// # Safety
    /// `cfg` must be null or point to a valid `MouseHmdConfig`.
    #[no_mangle]
    pub unsafe extern "C" fn mousehmd_input_start(cfg: *const Config) -> bool {
        // SAFETY: the caller guarantees `cfg` is null or valid for reads.
        let Some(cfg) = (unsafe { cfg.as_ref() }) else {
            return false;
        };
        input::start(cfg.sanitized()).is_ok()
    }

    #[no_mangle]
    pub extern "C" fn mousehmd_input_stop() {
        input::stop();
    }

    /// Writes the headset window: its size from the `windowWidth` and
    /// `windowHeight` settings (0 = not set) or from the primary monitor, and
    /// the position SteamVR gives it. Returns false if neither the monitor nor
    /// its work area could be read; `out` then holds the fallback size.
    ///
    /// # Safety
    /// `out` must be null or point to a writable `MouseHmdWindowRect`.
    #[no_mangle]
    pub unsafe extern "C" fn mousehmd_headset_window(
        width: i32,
        height: i32,
        out: *mut WindowRect,
    ) -> bool {
        // SAFETY: the caller guarantees `out` is null or valid for writes.
        let Some(out) = (unsafe { out.as_mut() }) else {
            return false;
        };
        let monitor = window::primary_monitor();
        let work = window::primary_work_area();
        *out = logic::headset_window(monitor, work, width, height);
        monitor.is_some() || work.is_some()
    }

    /// Writes the current head rotation as a quaternion `[w, x, y, z]`.
    ///
    /// # Safety
    /// `out_wxyz` must be null or point to four writable `f64`s.
    #[no_mangle]
    pub unsafe extern "C" fn mousehmd_get_rotation(out_wxyz: *mut f64) {
        if out_wxyz.is_null() {
            return;
        }
        let q = input::rotation();
        // SAFETY: non-null, and the caller guarantees room for four `f64`s.
        unsafe { std::ptr::copy_nonoverlapping(q.as_ptr(), out_wxyz, 4) };
    }

    /// Whether the key standing in for the controller system button is held.
    #[no_mangle]
    pub extern "C" fn mousehmd_system_button() -> bool {
        input::system_button()
    }

    /// Clamps the display settings in place to values that are safe to hand to SteamVR.
    ///
    /// # Safety
    /// `display` must be null or point to a valid, writable `MouseHmdDisplayConfig`.
    #[no_mangle]
    pub unsafe extern "C" fn mousehmd_sanitize_display(display: *mut DisplayConfig) {
        // SAFETY: the caller guarantees `display` is null or valid for reads and writes.
        if let Some(display) = unsafe { display.as_mut() } {
            *display = display.sanitized();
        }
    }
}
