//! Logs every change in whether the SteamVR dashboard is visible.
//!
//! Connects to an already running SteamVR as a background app and asks the
//! runtime itself (`IVROverlay::IsDashboardVisible`) a few times a second, so
//! "did that key press open or close the dashboard" has an answer that does not
//! depend on anyone watching the screen. The same text is written to
//! `dashboardwatch.log` in the current directory, line by line as it happens, so
//! the log is complete up to the last change even if the tool is stopped early.
//!
//! Usage: dashboardwatch [seconds] [path\to\openvr_api.dll]
//!
//! Exits with a non-zero status if it cannot connect or cannot write the log.

#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::{c_char, c_void, CStr};
use std::fs::File;
use std::io::Write as _;
use std::process::ExitCode;
use std::time::{Duration, Instant};

const DEFAULT_DLL: &str =
    r"C:\Program Files (x86)\Steam\steamapps\common\SteamVR\bin\win64\openvr_api.dll";
const OVERLAY_FNTABLE: &[u8] = b"FnTable:IVROverlay_028\0";
const VR_APPLICATION_BACKGROUND: i32 = 3;
/// Slot of `IsDashboardVisible` in `VR_IVROverlay_FnTable`, counted from
/// openvr_capi.h of OpenVR SDK 2.15.6, interface `IVROverlay_028`. Re-derive it
/// from the matching header whenever the interface string above changes.
const IS_DASHBOARD_VISIBLE_SLOT: usize = 68;
const LOG_FILE: &str = "dashboardwatch.log";
const POLL_INTERVAL: Duration = Duration::from_millis(50);

type InitInternal = unsafe extern "C" fn(*mut i32, i32) -> isize;
type ShutdownInternal = unsafe extern "C" fn();
type GetGenericInterface = unsafe extern "C" fn(*const c_char, *mut i32) -> *mut c_void;
type ErrorDescription = unsafe extern "C" fn(i32) -> *const c_char;
type IsDashboardVisible = unsafe extern "system" fn() -> bool;

/// Sends each line to the console and, immediately, to the log file.
#[derive(Default)]
struct Report {
    file: Option<File>,
    /// First error hit while writing the log, if any.
    write_error: Option<String>,
}

impl Report {
    fn to_file(path: &str) -> Result<Self, String> {
        let file = File::create(path).map_err(|e| format!("could not create {path}: {e}"))?;
        Ok(Self {
            file: Some(file),
            write_error: None,
        })
    }

    fn line(&mut self, line: &str) {
        println!("{line}");
        if let Some(file) = &mut self.file {
            // Flush per line: the point of this log is to survive an early stop.
            if let Err(e) = writeln!(file, "{line}").and_then(|()| file.flush()) {
                self.write_error.get_or_insert_with(|| e.to_string());
            }
        }
    }
}

fn state_line(elapsed_s: f64, visible: bool, first: bool) -> String {
    let state = if visible { "VISIBLE" } else { "hidden" };
    let what = if first {
        "dashboard is"
    } else {
        "dashboard became"
    };
    format!("t={elapsed_s:6.2}s  {what} {state}")
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);
    let dll = args.next().unwrap_or_else(|| DEFAULT_DLL.to_string());

    let mut report = match Report::to_file(LOG_FILE) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut ok = true;
    if let Err(e) = watch(seconds, &dll, &mut report) {
        report.line(&format!("error: {e}"));
        ok = false;
    }
    if let Some(e) = &report.write_error {
        eprintln!("error: could not write {LOG_FILE}: {e}");
        ok = false;
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn watch(seconds: u64, dll: &str, report: &mut Report) -> Result<(), String> {
    // SAFETY: loading a DLL runs its initialisers; this is SteamVR's own client library.
    let lib =
        unsafe { libloading::Library::new(dll) }.map_err(|e| format!("loading {dll}: {e}"))?;
    // SAFETY: each symbol is looked up with the signature openvr_capi.h declares for it.
    let (init, shutdown, get_interface, describe) = unsafe {
        let init: libloading::Symbol<InitInternal> =
            lib.get(b"VR_InitInternal\0").map_err(|e| e.to_string())?;
        let shutdown: libloading::Symbol<ShutdownInternal> = lib
            .get(b"VR_ShutdownInternal\0")
            .map_err(|e| e.to_string())?;
        let get_interface: libloading::Symbol<GetGenericInterface> = lib
            .get(b"VR_GetGenericInterface\0")
            .map_err(|e| e.to_string())?;
        let describe: libloading::Symbol<ErrorDescription> = lib
            .get(b"VR_GetVRInitErrorAsEnglishDescription\0")
            .map_err(|e| e.to_string())?;
        (init, shutdown, get_interface, describe)
    };
    // SAFETY: the function returns a pointer to a static, NUL-terminated string.
    let explain = |code: i32| {
        unsafe { CStr::from_ptr(describe(code)) }
            .to_string_lossy()
            .into_owned()
    };

    let mut err = 0i32;
    // SAFETY: `err` is a valid out-pointer for the duration of the call.
    unsafe { init(&mut err, VR_APPLICATION_BACKGROUND) };
    if err != 0 {
        return Err(format!(
            "VR_InitInternal failed ({err}): {}. Is SteamVR running?",
            explain(err)
        ));
    }

    // SAFETY: the name is NUL-terminated and `err` is a valid out-pointer.
    let table =
        unsafe { get_interface(OVERLAY_FNTABLE.as_ptr().cast(), &mut err) } as *const *const c_void;
    if table.is_null() || err != 0 {
        // SAFETY: balances the successful init above.
        unsafe { shutdown() };
        return Err(format!(
            "IVROverlay_028 not available ({err}): {}",
            explain(err)
        ));
    }
    // SAFETY: a non-null "FnTable:" interface is an array of function pointers;
    // slot and signature are those of IVROverlay_028 (see IS_DASHBOARD_VISIBLE_SLOT).
    let is_dashboard_visible: IsDashboardVisible =
        unsafe { std::mem::transmute(*table.add(IS_DASHBOARD_VISIBLE_SLOT)) };

    report.line(&format!("dashboardwatch: watching for {seconds} s"));
    let start = Instant::now();
    let mut last: Option<bool> = None;
    let mut changes = 0u32;
    while start.elapsed() < Duration::from_secs(seconds) {
        // SAFETY: takes no arguments and only reads runtime state.
        let visible = unsafe { is_dashboard_visible() };
        if last != Some(visible) {
            report.line(&state_line(
                start.elapsed().as_secs_f64(),
                visible,
                last.is_none(),
            ));
            changes += u32::from(last.is_some());
            last = Some(visible);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    // SAFETY: balances the successful init above; no OpenVR calls follow.
    unsafe { shutdown() };
    report.line(&format!("done: {changes} change(s)"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_lines_distinguish_the_initial_state_from_changes() {
        assert_eq!(
            state_line(0.0, true, true),
            "t=  0.00s  dashboard is VISIBLE"
        );
        assert_eq!(
            state_line(12.345, false, false),
            "t= 12.35s  dashboard became hidden"
        );
    }

    #[test]
    fn each_line_reaches_the_log_file_at_once() {
        let path =
            std::env::temp_dir().join(format!("dashboardwatch-test-{}.log", std::process::id()));
        let path_str = path.to_str().unwrap();
        let mut report = Report::to_file(path_str).unwrap();
        report.line("first");
        // Read back while the report is still open, as after a forced stop.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\n");
        report.line("second");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\nsecond\n");
        assert!(report.write_error.is_none());
        drop(report);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_dll_is_an_error() {
        let mut report = Report::default();
        let err = watch(1, "no-such-dir/openvr_api.dll", &mut report).err();
        assert!(err.is_some_and(|e| e.contains("loading no-such-dir/openvr_api.dll")));
    }
}
