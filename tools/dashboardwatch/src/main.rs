//! Logs every change in whether the SteamVR dashboard is visible.
//!
//! Connects to an already running SteamVR as a background app and asks the
//! runtime itself (`IVROverlay::IsDashboardVisible`) a few times a second, so
//! "did that key press open or close the dashboard" has an answer that does not
//! depend on anyone watching the screen. The same text is written to
//! `dashboardwatch.log` in the current directory, line by line as it happens, so
//! the log is complete up to the last change even if the tool is stopped early.
//!
//! Usage: dashboardwatch [--open] [seconds] [path\to\openvr_api.dll]
//!
//! With `--open`, the tool first asks SteamVR to show the dashboard
//! (`IVROverlay::ShowDashboard`), so that closing it can be tried under the same
//! conditions every time. It then connects as an overlay app, the kind of app
//! that call is meant for, instead of as a background app. The call reports
//! nothing back and takes effect a little later, so the tool waits for the
//! dashboard to show before it starts watching: the opening is then neither
//! missed nor counted as one of the changes.
//!
//! Exits with a non-zero status if it cannot connect, cannot write the log, or,
//! with `--open`, the dashboard does not show within `OPEN_TIMEOUT`.

#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::{c_char, c_void, CStr};
use std::fs::File;
use std::io::Write as _;
use std::process::ExitCode;
use std::time::{Duration, Instant};

const DEFAULT_DLL: &str =
    r"C:\Program Files (x86)\Steam\steamapps\common\SteamVR\bin\win64\openvr_api.dll";
const OVERLAY_FNTABLE: &[u8] = b"FnTable:IVROverlay_028\0";
const VR_APPLICATION_OVERLAY: i32 = 2;
const VR_APPLICATION_BACKGROUND: i32 = 3;
/// Slot of `IsDashboardVisible` in `VR_IVROverlay_FnTable`, counted from
/// openvr_capi.h of OpenVR SDK 2.15.6, interface `IVROverlay_028`. Re-derive it
/// from the matching header whenever the interface string above changes.
const IS_DASHBOARD_VISIBLE_SLOT: usize = 68;
/// Slot of `ShowDashboard` in the same table, counted the same way.
const SHOW_DASHBOARD_SLOT: usize = 72;
const LOG_FILE: &str = "dashboardwatch.log";
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// How long `--open` waits for the dashboard to show before giving up.
const OPEN_TIMEOUT: Duration = Duration::from_secs(5);

type InitInternal = unsafe extern "C" fn(*mut i32, i32) -> isize;
type ShutdownInternal = unsafe extern "C" fn();
type GetGenericInterface = unsafe extern "C" fn(*const c_char, *mut i32) -> *mut c_void;
type ErrorDescription = unsafe extern "C" fn(i32) -> *const c_char;
type IsDashboardVisible = unsafe extern "system" fn() -> bool;
type ShowDashboard = unsafe extern "system" fn(*mut c_char);

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

/// Follows the dashboard's visibility from one observation to the next.
#[derive(Default)]
struct Tracker {
    last: Option<bool>,
    /// Changes seen after the first observation.
    changes: u32,
}

impl Tracker {
    /// Records one observation. Returns `Some(first)` when it is worth a log
    /// line: `Some(true)` for the first observation, `Some(false)` for a change.
    fn observe(&mut self, visible: bool) -> Option<bool> {
        if self.last == Some(visible) {
            return None;
        }
        let first = self.last.is_none();
        self.changes += u32::from(!first);
        self.last = Some(visible);
        Some(first)
    }
}

/// Asks `is_visible` up to `polls` times, calling `pause` between two asks.
/// Returns the number of the ask that saw the dashboard, or `None` if none did.
fn wait_until_visible(
    mut is_visible: impl FnMut() -> bool,
    polls: u32,
    mut pause: impl FnMut(),
) -> Option<u32> {
    for poll in 1..=polls {
        if is_visible() {
            return Some(poll);
        }
        if poll < polls {
            pause();
        }
    }
    None
}

/// Command line: `--open` may appear anywhere; the rest are positional.
#[derive(Debug, PartialEq)]
struct Args {
    open: bool,
    seconds: u64,
    dll: String,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Args {
    let mut open = false;
    let mut positional = Vec::new();
    for arg in args {
        if arg == "--open" {
            open = true;
        } else {
            positional.push(arg);
        }
    }
    let mut positional = positional.into_iter();
    Args {
        open,
        seconds: positional.next().and_then(|s| s.parse().ok()).unwrap_or(60),
        dll: positional.next().unwrap_or_else(|| DEFAULT_DLL.to_string()),
    }
}

fn main() -> ExitCode {
    let Args { open, seconds, dll } = parse_args(std::env::args().skip(1));

    let mut report = match Report::to_file(LOG_FILE) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut ok = true;
    if let Err(e) = watch(seconds, open, &dll, &mut report) {
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

fn watch(seconds: u64, open: bool, dll: &str, report: &mut Report) -> Result<(), String> {
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
    let app_type = if open {
        VR_APPLICATION_OVERLAY
    } else {
        VR_APPLICATION_BACKGROUND
    };
    // SAFETY: `err` is a valid out-pointer for the duration of the call.
    unsafe { init(&mut err, app_type) };
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

    if open {
        // SAFETY: slot and signature are those of IVROverlay_028 (see SHOW_DASHBOARD_SLOT).
        let show_dashboard: ShowDashboard =
            unsafe { std::mem::transmute(*table.add(SHOW_DASHBOARD_SLOT)) };
        // An empty overlay key asks for the dashboard as it is, not a particular overlay.
        let mut no_overlay = [0 as c_char];
        // SAFETY: a valid, NUL-terminated string that outlives the call.
        unsafe { show_dashboard(no_overlay.as_mut_ptr()) };
        report.line("requested ShowDashboard");

        let requested = Instant::now();
        let polls = (OPEN_TIMEOUT.as_millis() / POLL_INTERVAL.as_millis()) as u32;
        let shown = wait_until_visible(
            // SAFETY: takes no arguments and only reads runtime state.
            || unsafe { is_dashboard_visible() },
            polls,
            || std::thread::sleep(POLL_INTERVAL),
        );
        if shown.is_none() {
            // SAFETY: balances the successful init above; no OpenVR calls follow.
            unsafe { shutdown() };
            return Err(format!(
                "the dashboard did not show within {} s of ShowDashboard. \
                 Is it disabled (dashboard.enableDashboard)?",
                OPEN_TIMEOUT.as_secs()
            ));
        }
        report.line(&format!(
            "dashboard shown {:.2} s after the request",
            requested.elapsed().as_secs_f64()
        ));
    }

    report.line(&format!("dashboardwatch: watching for {seconds} s"));
    let start = Instant::now();
    let mut tracker = Tracker::default();
    if open {
        // The dashboard was just seen showing. Start from that, so that a close
        // before the first poll below still counts as a change.
        tracker.observe(true);
        report.line(&state_line(0.0, true, true));
    }
    while start.elapsed() < Duration::from_secs(seconds) {
        // SAFETY: takes no arguments and only reads runtime state.
        let visible = unsafe { is_dashboard_visible() };
        if let Some(first) = tracker.observe(visible) {
            report.line(&state_line(start.elapsed().as_secs_f64(), visible, first));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    let changes = tracker.changes;
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

    fn track(observations: &[bool]) -> (Vec<Option<bool>>, u32) {
        let mut tracker = Tracker::default();
        let lines = observations.iter().map(|&v| tracker.observe(v)).collect();
        (lines, tracker.changes)
    }

    #[test]
    fn tracker_logs_the_first_state_and_counts_only_changes() {
        assert_eq!(
            track(&[false, false, true, true, false]),
            (vec![Some(true), None, Some(false), None, Some(false)], 2)
        );
    }

    #[test]
    fn a_close_right_after_open_is_a_change() {
        // --open starts from the visible state it confirmed; the first poll
        // already finds the dashboard closed.
        assert_eq!(track(&[true, false]), (vec![Some(true), Some(false)], 1));
    }

    /// Runs `wait_until_visible` against a scripted sequence of answers.
    /// Returns its result and how many asks and pauses it made.
    fn wait_with(answers: &[bool], polls: u32) -> (Option<u32>, usize, usize) {
        let mut asked = 0;
        let mut paused = 0;
        let result = wait_until_visible(
            || {
                asked += 1;
                answers[asked - 1]
            },
            polls,
            || paused += 1,
        );
        (result, asked, paused)
    }

    #[test]
    fn an_already_visible_dashboard_needs_one_ask() {
        assert_eq!(wait_with(&[true], 100), (Some(1), 1, 0));
    }

    #[test]
    fn waiting_stops_as_soon_as_the_dashboard_shows() {
        assert_eq!(wait_with(&[false, false, true, true], 100), (Some(3), 3, 2));
    }

    #[test]
    fn a_dashboard_that_never_shows_gives_up_after_the_last_ask() {
        assert_eq!(wait_with(&[false; 4], 4), (None, 4, 3));
    }

    fn args(list: &[&str]) -> Args {
        parse_args(list.iter().map(|s| s.to_string()))
    }

    #[test]
    fn open_flag_may_appear_anywhere() {
        let expected = Args {
            open: true,
            seconds: 30,
            dll: "x.dll".into(),
        };
        assert_eq!(args(&["--open", "30", "x.dll"]), expected);
        assert_eq!(args(&["30", "--open", "x.dll"]), expected);
        assert_eq!(args(&["30", "x.dll", "--open"]), expected);
    }

    #[test]
    fn defaults_without_arguments() {
        let a = args(&[]);
        assert!(!a.open);
        assert_eq!(a.seconds, 60);
        assert_eq!(a.dll, DEFAULT_DLL);
    }

    #[test]
    fn missing_dll_is_an_error() {
        let mut report = Report::default();
        let err = watch(1, false, "no-such-dir/openvr_api.dll", &mut report).err();
        assert!(err.is_some_and(|e| e.contains("loading no-such-dir/openvr_api.dll")));
    }
}
