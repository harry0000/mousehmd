//! Polls global mouse/keyboard state and turns it into yaw/pitch.
//!
//! The driver is loaded into vrserver.exe, which has no window of its own, so
//! input is read with `GetAsyncKeyState` / `GetCursorPos` rather than window
//! messages. To avoid hijacking the mouse everywhere, input only counts while
//! one of the VR windows is in the foreground (unless `any_window` is set). The
//! keys count only in the VR app's window; SteamVR's own windows take the mouse only.
//!
//! Controls:
//!   right mouse button + drag   look around
//!   middle mouse button         recenter
//!   I / K / J / L               look up / down / left / right
//!   H                           recenter
//!   M                           system button (opens / closes the SteamVR dashboard)
//!
//! The keys deliberately avoid Ctrl, Alt, Shift, Space, Enter, the arrow keys and
//! WASD: those are Unity's default input bindings (Fire1-3, Jump, Submit and the
//! movement axes), and the app in the foreground receives every key we read.
//! For the same reason the look keys are ignored while a modifier is held, so
//! that the app's own shortcuts (Ctrl+H, Alt+I, ...) do not also move the view.

use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, HWND, POINT};
use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_H, VK_I, VK_J, VK_K, VK_L, VK_LWIN, VK_M, VK_MBUTTON, VK_MENU,
    VK_RBUTTON, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId,
};

use crate::logic::{self, Config, InputScope, Keys};

/// The VR app. Its window takes both the mouse and the keys.
/// Matched by executable file name (ASCII case-insensitive).
const APP_PROCESSES: &[&str] = &["dmmvrplayer_windows.exe"];
/// SteamVR's own windows. Only the mouse counts there: they have keyboard
/// shortcuts of their own, which our keys would trigger as well.
const STEAMVR_PROCESSES: &[&str] = &["vrcompositor.exe", "vrserver.exe", "vrmonitor.exe"];

const POLL_INTERVAL: Duration = Duration::from_millis(2);
const MAX_PITCH_DEG: f64 = 89.0;

// Orientation in degrees, stored as f64 bit patterns so the pose thread can read lock-free.
static YAW_DEG: AtomicU64 = AtomicU64::new(0);
static PITCH_DEG: AtomicU64 = AtomicU64::new(0);

// Whether the stand-in system button is held, for the pose thread to forward to SteamVR.
static SYSTEM_BUTTON: AtomicBool = AtomicBool::new(false);

static RUNNING: AtomicBool = AtomicBool::new(false);
static THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// Starts the input thread. Does nothing if it is already running. If the OS
/// refuses to create the thread the error is returned and the state is left
/// stopped, so a later call can try again.
pub fn start(cfg: Config) -> io::Result<()> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    store(0.0, 0.0);
    match thread::Builder::new()
        .name("mousehmd-input".into())
        .spawn(move || poll_loop(cfg))
    {
        Ok(handle) => {
            *thread_slot() = Some(handle);
            Ok(())
        }
        Err(e) => {
            RUNNING.store(false, Ordering::SeqCst);
            Err(e)
        }
    }
}

pub fn stop() {
    RUNNING.store(false, Ordering::SeqCst);
    SYSTEM_BUTTON.store(false, Ordering::Relaxed);
    let handle = thread_slot().take();
    if let Some(handle) = handle {
        let _ = handle.join();
    }
}

/// The slot only ever holds a join handle, so a poisoned lock is still usable.
/// Panicking here would take vrserver down with us (`panic = "abort"`).
fn thread_slot() -> MutexGuard<'static, Option<JoinHandle<()>>> {
    THREAD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether the stand-in system button is currently held.
pub fn system_button() -> bool {
    SYSTEM_BUTTON.load(Ordering::Relaxed)
}

/// Current head rotation as a quaternion `[w, x, y, z]`.
pub fn rotation() -> [f64; 4] {
    logic::rotation_from_yaw_pitch(
        f64::from_bits(YAW_DEG.load(Ordering::Relaxed)),
        f64::from_bits(PITCH_DEG.load(Ordering::Relaxed)),
    )
}

fn store(yaw: f64, pitch: f64) {
    YAW_DEG.store(yaw.to_bits(), Ordering::Relaxed);
    PITCH_DEG.store(pitch.to_bits(), Ordering::Relaxed);
}

fn key_down(vk: u16) -> bool {
    // High bit set = currently down.
    unsafe { GetAsyncKeyState(vk as i32) < 0 }
}

fn read_keys() -> Keys {
    Keys {
        look_up: key_down(VK_I),
        look_down: key_down(VK_K),
        look_left: key_down(VK_J),
        look_right: key_down(VK_L),
        recenter: key_down(VK_H),
        system: key_down(VK_M),
        modifier: [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
            .into_iter()
            .any(key_down),
    }
}

fn cursor_pos() -> Option<(i32, i32)> {
    let mut p = POINT { x: 0, y: 0 };
    (unsafe { GetCursorPos(&mut p) } != 0).then_some((p.x, p.y))
}

/// Caches the scope per foreground window so we only query the process on focus changes.
struct ForegroundFilter {
    any_window: bool,
    last_hwnd: HWND,
    last_scope: InputScope,
}

impl ForegroundFilter {
    fn new(any_window: bool) -> Self {
        Self {
            any_window,
            last_hwnd: std::ptr::null_mut(),
            last_scope: InputScope::None,
        }
    }

    fn scope(&mut self) -> InputScope {
        if self.any_window {
            return InputScope::MouseAndKeys;
        }
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return InputScope::None;
        }
        if hwnd != self.last_hwnd {
            self.last_hwnd = hwnd;
            self.last_scope = process_path_of(hwnd)
                .map(|path| logic::input_scope(&path, APP_PROCESSES, STEAMVR_PROCESSES))
                .unwrap_or(InputScope::None);
        }
        self.last_scope
    }
}

fn process_path_of(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

fn poll_loop(cfg: Config) {
    // Windows sleeps in ~15.6 ms steps by default, which would make the pose
    // advance unevenly against a 60 Hz display. Ask for 1 ms for this process.
    unsafe { timeBeginPeriod(1) };
    run(cfg);
    unsafe { timeEndPeriod(1) };
}

fn run(cfg: Config) {
    let mouse_gain = f64::from(cfg.mouse_deg_per_px) * if cfg.grab_style { 1.0 } else { -1.0 };
    let key_speed = f64::from(cfg.key_deg_per_sec);
    let smoothing_secs = f64::from(cfg.smoothing_ms.max(0.0)) / 1000.0;

    let mut filter = ForegroundFilter::new(cfg.any_window);
    // Where the input says we should look. Yaw is left unwrapped so the filter
    // never has to cross a +-180 seam. Positive yaw = left, positive pitch = up.
    let mut target_yaw = 0.0f64;
    let mut target_pitch = 0.0f64;
    // What we report: the target run through a low-pass filter. Mouse input
    // arrives as whole pixels at the mouse's own rate; unfiltered, a slow drag
    // shows up as visible steps.
    let mut yaw = 0.0f64;
    let mut pitch = 0.0f64;
    let mut drag_origin: Option<(i32, i32)> = None;
    let mut last_tick = Instant::now();

    while RUNNING.load(Ordering::Relaxed) {
        thread::sleep(POLL_INTERVAL);
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f64();
        last_tick = now;

        let mut recenter = false;
        let scope = filter.scope();

        // Mouse: in the VR app and in SteamVR's own windows.
        if scope == InputScope::None {
            drag_origin = None;
        } else {
            if key_down(VK_RBUTTON) {
                if let Some((x, y)) = cursor_pos() {
                    if let Some((ox, oy)) = drag_origin {
                        // Grab style: dragging right pulls the picture right, i.e. the head turns left.
                        target_yaw += f64::from(x - ox) * mouse_gain;
                        target_pitch += f64::from(y - oy) * mouse_gain;
                    }
                    drag_origin = Some((x, y));
                }
            } else {
                drag_origin = None;
            }
            recenter |= key_down(VK_MBUTTON);
        }

        // Keys: in the VR app only.
        if scope == InputScope::MouseAndKeys {
            let keys = read_keys();
            let look = logic::key_look(keys);
            let step = key_speed * dt;
            target_yaw += look.yaw_direction * step;
            target_pitch += look.pitch_direction * step;
            recenter |= look.recenter;
            SYSTEM_BUTTON.store(logic::system_button(keys), Ordering::Relaxed);
        } else {
            // Leaving the app's window releases the button, so it can never stay stuck down.
            SYSTEM_BUTTON.store(false, Ordering::Relaxed);
        }

        if recenter {
            // Fold accumulated turns away so the view returns by the short way round.
            yaw = (yaw + 180.0).rem_euclid(360.0) - 180.0;
            target_yaw = 0.0;
            target_pitch = 0.0;
        }
        target_pitch = target_pitch.clamp(-MAX_PITCH_DEG, MAX_PITCH_DEG);

        let blend = if smoothing_secs > 0.0 {
            1.0 - (-dt / smoothing_secs).exp()
        } else {
            1.0
        };
        yaw += (target_yaw - yaw) * blend;
        pitch += (target_pitch - pitch) * blend;
        store(yaw, pitch);
    }
}
