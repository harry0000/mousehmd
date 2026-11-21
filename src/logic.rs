//! Platform-independent decisions, kept apart from the Win32 calls so they can be tested.

/// Largest window or render-target edge we will hand to SteamVR, in pixels.
pub const MAX_DIMENSION: i32 = 16384;
pub const MIN_DIMENSION: i32 = 64;

/// Mirrors `MouseHmdConfig` in `cpp/shim.cpp`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    /// Degrees of rotation per pixel of mouse drag.
    pub mouse_deg_per_px: f32,
    /// Degrees per second while a look key is held.
    pub key_deg_per_sec: f32,
    /// true: drag moves the picture (Street View style). false: FPS style.
    pub grab_style: bool,
    /// true: react regardless of which window is in the foreground.
    pub any_window: bool,
    /// Time constant of the low-pass filter on the view direction, in ms. 0 disables it.
    pub smoothing_ms: f32,
}

impl Config {
    /// Replaces values that would break the orientation maths. A NaN gain would
    /// otherwise turn the view direction into NaN for the rest of the session.
    pub fn sanitized(self) -> Self {
        Self {
            mouse_deg_per_px: finite_clamped(self.mouse_deg_per_px, 0.12, 0.0, 10.0),
            key_deg_per_sec: finite_clamped(self.key_deg_per_sec, 60.0, 0.0, 720.0),
            smoothing_ms: finite_clamped(self.smoothing_ms, 0.0, 0.0, 1000.0),
            ..self
        }
    }
}

/// Mirrors `MouseHmdDisplayConfig` in `cpp/shim.cpp`. Dimensions stay signed so
/// that a negative setting is seen as negative, not as a huge unsigned number.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayConfig {
    pub window_width: i32,
    pub window_height: i32,
    pub horizontal_fov_deg: f32,
    pub render_scale: f32,
    pub display_frequency: f32,
    pub ipd_meters: f32,
    pub head_height_meters: f32,
}

impl DisplayConfig {
    /// Brings every value into a range SteamVR can safely be given. After this,
    /// both window edges and both edges scaled by `render_scale` fit in
    /// `MIN_DIMENSION..=MAX_DIMENSION`.
    pub fn sanitized(self) -> Self {
        let window_width = self.window_width.clamp(MIN_DIMENSION, MAX_DIMENSION);
        let window_height = self.window_height.clamp(MIN_DIMENSION, MAX_DIMENSION);
        let longest = window_width.max(window_height) as f32;
        let max_scale = (MAX_DIMENSION as f32 / longest).min(4.0);
        Self {
            window_width,
            window_height,
            horizontal_fov_deg: finite_clamped(self.horizontal_fov_deg, 100.0, 20.0, 160.0),
            render_scale: finite_clamped(self.render_scale, 1.0, 0.25, max_scale.max(0.25)),
            display_frequency: finite_clamped(self.display_frequency, 60.0, 10.0, 1000.0),
            ipd_meters: finite_clamped(self.ipd_meters, 0.063, 0.0, 0.2),
            head_height_meters: finite_clamped(self.head_height_meters, 0.0, -10.0, 10.0),
        }
    }
}

fn finite_clamped(value: f32, fallback: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

/// Whether `process_path` is one of `targets`, compared by executable file name
/// only. A folder that merely contains a target's name does not count.
pub fn is_target_executable(process_path: &str, targets: &[&str]) -> bool {
    // Split by hand: the path is a Windows path even when tests run elsewhere.
    let file_name = process_path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(process_path);
    targets
        .iter()
        .any(|target| file_name.eq_ignore_ascii_case(target))
}

/// Which window is in the foreground, as far as the input is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputScope {
    /// Not one of ours: ignore everything.
    None,
    /// One of SteamVR's own windows. They have keyboard shortcuts of their own
    /// (M toggles the legacy mirror, for one), so the mouse counts here but of
    /// the keys only the SteamVR system key does.
    SteamVr,
    /// The VR app: mouse, look keys and the app's system key.
    App,
    /// `anyWindow` is set: everything counts, whatever is in the foreground.
    Anywhere,
}

/// Decides the scope from the foreground window's executable path.
pub fn input_scope(
    process_path: &str,
    app_targets: &[&str],
    steamvr_targets: &[&str],
) -> InputScope {
    if is_target_executable(process_path, app_targets) {
        InputScope::App
    } else if is_target_executable(process_path, steamvr_targets) {
        InputScope::SteamVr
    } else {
        InputScope::None
    }
}

/// Whether the mouse moves the view in this scope.
pub fn takes_mouse(scope: InputScope) -> bool {
    scope != InputScope::None
}

/// Whether the look keys move the view in this scope.
pub fn takes_look_keys(scope: InputScope) -> bool {
    matches!(scope, InputScope::App | InputScope::Anywhere)
}

/// Which keys are down right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    pub look_up: bool,
    pub look_down: bool,
    pub look_left: bool,
    pub look_right: bool,
    pub recenter: bool,
    /// The key standing in for a controller's system button in the VR app (M).
    pub system: bool,
    /// The same, in SteamVR's own windows (F9), where M toggles the legacy mirror.
    pub system_steamvr: bool,
    /// Ctrl, Alt, Shift or the Windows key.
    pub modifier: bool,
}

/// What the keyboard asks of the view this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct KeyLook {
    /// +1 = turn left, -1 = turn right.
    pub yaw_direction: f64,
    /// +1 = look up, -1 = look down.
    pub pitch_direction: f64,
    pub recenter: bool,
}

/// The look keys only count when pressed on their own. With a modifier held the
/// press is a shortcut meant for the app in the foreground (Ctrl+H, Alt+I, ...).
pub fn key_look(keys: Keys) -> KeyLook {
    if keys.modifier {
        return KeyLook::default();
    }
    let axis = |positive: bool, negative: bool| f64::from(i8::from(positive) - i8::from(negative));
    KeyLook {
        yaw_direction: axis(keys.look_left, keys.look_right),
        pitch_direction: axis(keys.look_up, keys.look_down),
        recenter: keys.recenter,
    }
}

/// The keys that stand in for the system button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SystemKey {
    /// M, in the VR app.
    App,
    /// F9, in SteamVR's own windows.
    SteamVr,
}

impl SystemKey {
    const ALL: [SystemKey; 2] = [SystemKey::App, SystemKey::SteamVr];

    fn is_down(self, keys: Keys) -> bool {
        match self {
            SystemKey::App => keys.system,
            SystemKey::SteamVr => keys.system_steamvr,
        }
    }

    /// Each kind of window has its own key, and with `anyWindow` either counts.
    fn counts_in(self, scope: InputScope) -> bool {
        matches!(
            (scope, self),
            (InputScope::App, SystemKey::App)
                | (InputScope::SteamVr, SystemKey::SteamVr)
                | (InputScope::Anywhere, _)
        )
    }
}

/// Turns the system keys into the stand-in system button.
///
/// SteamVR toggles the dashboard each time the button goes down, so the button
/// may only go down when a key is pressed afresh: in a window where it counts,
/// with no modifier held, after the key was seen up on the previous tick. A key
/// that is already down never presses the button, whether a modifier was just
/// let go (Ctrl+F9 with Ctrl released first) or the key was held while moving
/// in from another window. Keys are not watched outside our windows, so after
/// the foreground has been elsewhere a key must first be seen up.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemButton {
    /// Whether each key was down on the last tick it was watched; `None` when
    /// it was not watched (at start, or while another window was in front).
    last_down: [Option<bool>; 2],
    /// The key holding the button down.
    held_by: Option<SystemKey>,
}

impl SystemButton {
    /// Advances one tick and returns whether the button is held.
    pub fn update(&mut self, scope: InputScope, keys: Keys) -> bool {
        // The button stays down while its key does, in a window where it counts.
        if let Some(key) = self.held_by {
            if !(key.counts_in(scope) && key.is_down(keys)) {
                self.held_by = None;
            }
        }
        if self.held_by.is_none() && !keys.modifier {
            self.held_by = SystemKey::ALL.into_iter().enumerate().find_map(|(i, key)| {
                let fresh = key.is_down(keys) && self.last_down[i] == Some(false);
                (fresh && key.counts_in(scope)).then_some(key)
            });
        }
        for (i, key) in SystemKey::ALL.into_iter().enumerate() {
            self.last_down[i] = (scope != InputScope::None).then(|| key.is_down(keys));
        }
        self.held_by.is_some()
    }
}

/// Head rotation as a quaternion `[w, x, y, z]` in OpenVR's coordinate system
/// (+Y up, -Z forward): yaw about +Y, then pitch about the rotated +X.
/// Positive yaw turns left, positive pitch looks up.
pub fn rotation_from_yaw_pitch(yaw_deg: f64, pitch_deg: f64) -> [f64; 4] {
    let (sy, cy) = (yaw_deg.to_radians() / 2.0).sin_cos();
    let (sp, cp) = (pitch_deg.to_radians() / 2.0).sin_cos();
    [cy * cp, cy * sp, sy * cp, -sy * sp]
}

/// A window rectangle in physical desktop pixels. Mirrors `MouseHmdWindowRect`
/// in `cpp/shim.cpp`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The default size when neither the monitor nor the work area can be read.
pub const FALLBACK_SIZE: (i32, i32) = (1280, 720);
/// The default window is at least this wide (960x540) when that fits the
/// work area, so that small screens do not get a tiny window.
const MIN_DEFAULT_UNIT: i32 = 60;

/// The headset window's size when it is not set: the largest 16:9 size
/// within half of the whole primary monitor in each direction, which leaves
/// room for the VR app's own window, the one mainly watched. Half of the whole
/// monitor rather than of the work area, so that a 16:9 monitor gets exactly
/// half (3840x2160 gives 1920x1080) whatever the taskbar takes. On a small or
/// portrait screen half would be too small to read, so it grows to 960x540
/// where that fits; it never grows past the work area.
pub fn default_size(monitor: Option<WindowRect>, work: Option<WindowRect>) -> (i32, i32) {
    let Some(base) = monitor.or(work) else {
        return FALLBACK_SIZE;
    };
    let room = work.unwrap_or(base);
    // 16:9 in whole units, so the ratio is exact. 8 units keeps both edges
    // above MIN_DIMENSION even on a tiny screen.
    let half = (base.width / 2 / 16).min(base.height / 2 / 9);
    let fits = (room.width / 16).min(room.height / 9);
    let unit = half.max(MIN_DEFAULT_UNIT.min(fits)).min(fits).max(8);
    (16 * unit, 9 * unit)
}

/// The headset window's size from the settings, where 0 means not set. If
/// only one edge is set, the other follows at 16:9 so the picture keeps its
/// proportions; if neither is, `default` is used. The result is clamped like
/// the other display settings.
pub fn window_size(default: (i32, i32), width: i32, height: i32) -> (i32, i32) {
    let (width, height) = match (width, height) {
        (0, 0) => default,
        (w, 0) => (w, (i64::from(w) * 9 / 16) as i32),
        (0, h) => ((i64::from(h) * 16 / 9).min(i64::from(i32::MAX)) as i32, h),
        (w, h) => (w, h),
    };
    (
        width.clamp(MIN_DIMENSION, MAX_DIMENSION),
        height.clamp(MIN_DIMENSION, MAX_DIMENSION),
    )
}

/// The headset window: its size from the settings or the screen, and the
/// position SteamVR gives it, which is the centre of the whole primary
/// monitor (taskbar included), whatever position the driver reports.
pub fn headset_window(
    monitor: Option<WindowRect>,
    work: Option<WindowRect>,
    width: i32,
    height: i32,
) -> WindowRect {
    let (width, height) = window_size(default_size(monitor, work), width, height);
    let monitor = monitor.or(work).unwrap_or(WindowRect {
        x: 0,
        y: 0,
        width,
        height,
    });
    WindowRect {
        x: monitor.x + (monitor.width - width) / 2,
        y: monitor.y + (monitor.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, width: i32, height: i32) -> WindowRect {
        WindowRect {
            x,
            y,
            width,
            height,
        }
    }

    /// `default_size` for a primary monitor of `width` x `height` with a
    /// taskbar of `taskbar` pixels along the bottom.
    fn default_for(width: i32, height: i32, taskbar: i32) -> (i32, i32) {
        default_size(
            Some(rect(0, 0, width, height)),
            Some(rect(0, 0, width, height - taskbar)),
        )
    }

    #[test]
    fn default_size_is_half_the_whole_monitor_at_16_9() {
        // 4K at 150%: the taskbar does not shrink the window below half.
        assert_eq!(default_for(3840, 2160, 72), (1920, 1080));
        assert_eq!(default_for(3840, 2160, 0), (1920, 1080));
        // 1440p.
        assert_eq!(default_for(2560, 1440, 48), (1280, 720));
        // Surface Pro (3:2): the width decides.
        assert_eq!(default_for(2736, 1824, 72), (1360, 765));
    }

    #[test]
    fn default_size_on_ultrawide_monitors_follows_the_height() {
        // 21:9 and 32:9: half the height decides, so the window does not
        // take most of the screen's height.
        assert_eq!(default_for(3440, 1440, 48), (1280, 720));
        assert_eq!(default_for(5120, 1440, 48), (1280, 720));
        // 2560x1080: half is 960x540 exactly.
        assert_eq!(default_for(2560, 1080, 40), (960, 540));
    }

    #[test]
    fn default_size_on_portrait_monitors_grows_to_960x540_within_the_width() {
        // Half the width would be tiny (528x297 on 1080x1920), so 960x540.
        assert_eq!(default_for(1080, 1920, 48), (960, 540));
        assert_eq!(default_for(1440, 2560, 48), (960, 540));
        // 4K portrait: half the width is already larger.
        assert_eq!(default_for(2160, 3840, 72), (1072, 603));
        // Too narrow for 960: as wide as the screen.
        assert_eq!(default_for(768, 1366, 40), (768, 432));
    }

    #[test]
    fn default_size_grows_to_960x540_on_small_screens_when_it_fits() {
        assert_eq!(default_for(1920, 1080, 40), (960, 540));
        assert_eq!(default_for(1366, 768, 40), (960, 540));
        // 800x600: 960x540 does not fit, so as large as fits.
        assert_eq!(default_for(800, 600, 40), (800, 450));
    }

    #[test]
    fn default_size_never_grows_past_the_work_area() {
        // A taskbar docked on the left that leaves 420 pixels of width.
        let size = default_size(Some(rect(0, 0, 1920, 1080)), Some(rect(1500, 0, 420, 1080)));
        assert_eq!(size, (416, 234));
    }

    #[test]
    fn default_size_falls_back_and_never_goes_below_the_minimum() {
        assert_eq!(default_size(None, None), FALLBACK_SIZE);
        // Without the monitor, the work area stands in for it.
        assert_eq!(
            default_size(None, Some(rect(0, 0, 3840, 2160))),
            (1920, 1080)
        );
        let (w, h) = default_size(Some(rect(0, 0, 0, 0)), None);
        assert_eq!((w, h), (128, 72));
        assert!(w >= MIN_DIMENSION && h >= MIN_DIMENSION);
    }

    #[test]
    fn window_size_uses_the_settings_and_fills_in_a_missing_edge_at_16_9() {
        assert_eq!(window_size((1856, 1044), 0, 0), (1856, 1044));
        assert_eq!(window_size((1856, 1044), 1600, 900), (1600, 900));
        assert_eq!(window_size((1856, 1044), 800, 600), (800, 600));
        // Only one edge set.
        assert_eq!(window_size((1856, 1044), 1280, 0), (1280, 720));
        assert_eq!(window_size((1856, 1044), 0, 720), (1280, 720));
        // Out of range values are clamped, even when derived.
        assert_eq!(
            window_size((1856, 1044), -5, 0),
            (MIN_DIMENSION, MIN_DIMENSION)
        );
        assert_eq!(
            window_size((1856, 1044), 0, i32::MAX),
            (MAX_DIMENSION, MAX_DIMENSION)
        );
    }

    #[test]
    fn headset_window_is_centred_on_the_whole_monitor() {
        // SteamVR centres on the whole monitor, taskbar included (a 1856x1044
        // window on this 4K monitor was found at (992, 558)).
        let monitor = Some(rect(0, 0, 3840, 2160));
        let work = Some(rect(0, 0, 3840, 2088));
        assert_eq!(
            headset_window(monitor, work, 0, 0),
            rect(960, 540, 1920, 1080)
        );
        assert_eq!(
            headset_window(monitor, work, 1856, 1044),
            rect(992, 558, 1856, 1044)
        );
        // An explicit size is centred the same way.
        assert_eq!(
            headset_window(monitor, work, 800, 600),
            rect(1520, 780, 800, 600)
        );
        // A portrait monitor.
        let portrait = Some(rect(0, 0, 1080, 1920));
        assert_eq!(
            headset_window(portrait, Some(rect(0, 0, 1080, 1872)), 0, 0),
            rect(60, 690, 960, 540)
        );
        // Without the monitor the work area stands in; without either, the
        // fallback size at the origin.
        assert_eq!(
            headset_window(None, Some(rect(0, 0, 1920, 1040)), 0, 0),
            rect(480, 250, 960, 540)
        );
        assert_eq!(headset_window(None, None, 0, 0), rect(0, 0, 1280, 720));
    }

    const TARGETS: &[&str] = &["dmmvrplayer_windows.exe", "vrcompositor.exe"];

    #[test]
    fn target_matches_executable_name_in_any_case() {
        assert!(is_target_executable(
            r"C:\Users\me\AppData\Local\DMMVRPlayer\DMMVRPlayer_Windows.exe",
            TARGETS
        ));
        assert!(is_target_executable(
            r"c:\steam\steamvr\bin\win64\VRCOMPOSITOR.EXE",
            TARGETS
        ));
        assert!(is_target_executable("vrcompositor.exe", TARGETS));
    }

    #[test]
    fn target_ignores_folder_names_and_partial_names() {
        assert!(!is_target_executable(
            r"C:\work\vrcompositor.exe\editor.exe",
            TARGETS
        ));
        assert!(!is_target_executable(
            r"C:\work\dmmvrplayer_windows-tools\editor.exe",
            TARGETS
        ));
        assert!(!is_target_executable(
            r"C:\tools\my-vrcompositor.exe",
            TARGETS
        ));
        assert!(!is_target_executable(
            r"C:\tools\vrcompositor.exe.bak",
            TARGETS
        ));
        assert!(!is_target_executable("", TARGETS));
    }

    #[test]
    fn scope_tells_the_app_from_steamvr_and_from_other_windows() {
        let app = &["dmmvrplayer_windows.exe"];
        let steamvr = &["vrcompositor.exe", "vrmonitor.exe"];
        let scope = |path| input_scope(path, app, steamvr);
        assert_eq!(scope(r"C:\Apps\DMMVRPlayer_Windows.exe"), InputScope::App);
        assert_eq!(
            scope(r"C:\SteamVR\bin\win64\vrcompositor.exe"),
            InputScope::SteamVr
        );
        assert_eq!(
            scope(r"C:\SteamVR\bin\win64\vrmonitor.exe"),
            InputScope::SteamVr
        );
        assert_eq!(scope(r"C:\Windows\notepad.exe"), InputScope::None);
        assert_eq!(scope(r"C:\vrcompositor.exe\notepad.exe"), InputScope::None);
    }

    #[test]
    fn mouse_counts_in_every_scope_but_none_and_look_keys_skip_steamvr() {
        use InputScope::*;
        assert!(!takes_mouse(None));
        assert!(takes_mouse(SteamVr) && takes_mouse(App) && takes_mouse(Anywhere));
        assert!(!takes_look_keys(None) && !takes_look_keys(SteamVr));
        assert!(takes_look_keys(App) && takes_look_keys(Anywhere));
    }

    fn keys(set: impl Fn(&mut Keys)) -> Keys {
        let mut k = Keys::default();
        set(&mut k);
        k
    }

    #[test]
    fn each_look_key_alone_moves_the_view() {
        assert_eq!(key_look(keys(|k| k.look_left = true)).yaw_direction, 1.0);
        assert_eq!(key_look(keys(|k| k.look_right = true)).yaw_direction, -1.0);
        assert_eq!(key_look(keys(|k| k.look_up = true)).pitch_direction, 1.0);
        assert_eq!(key_look(keys(|k| k.look_down = true)).pitch_direction, -1.0);
        assert!(key_look(keys(|k| k.recenter = true)).recenter);
        assert_eq!(key_look(Keys::default()), KeyLook::default());
    }

    #[test]
    fn opposite_keys_cancel() {
        let both = keys(|k| {
            k.look_left = true;
            k.look_right = true;
        });
        assert_eq!(key_look(both).yaw_direction, 0.0);
    }

    #[test]
    fn modifier_turns_look_keys_into_app_shortcuts() {
        let ctrl_h = keys(|k| {
            k.recenter = true;
            k.modifier = true;
        });
        assert_eq!(key_look(ctrl_h), KeyLook::default());
        let alt_i = keys(|k| {
            k.look_up = true;
            k.modifier = true;
        });
        assert_eq!(key_look(alt_i), KeyLook::default());
    }

    const M: Keys = Keys {
        look_up: false,
        look_down: false,
        look_left: false,
        look_right: false,
        recenter: false,
        system: true,
        system_steamvr: false,
        modifier: false,
    };
    const F9: Keys = Keys {
        system: false,
        system_steamvr: true,
        ..M
    };
    const NONE: Keys = Keys { system: false, ..M };

    fn with_modifier(k: Keys) -> Keys {
        Keys {
            modifier: true,
            ..k
        }
    }

    /// Feeds one tick per step and returns the button state after each.
    fn run_button(steps: &[(InputScope, Keys)]) -> Vec<bool> {
        let mut button = SystemButton::default();
        steps
            .iter()
            .map(|&(scope, keys)| button.update(scope, keys))
            .collect()
    }

    #[test]
    fn system_button_key_depends_on_the_window() {
        use InputScope::*;
        // M in the VR app, F9 in SteamVR's windows, either with anyWindow.
        for (scope, key, counts) in [
            (App, M, true),
            (App, F9, false),
            (SteamVr, F9, true),
            (SteamVr, M, false),
            (Anywhere, M, true),
            (Anywhere, F9, true),
            (None, M, false),
            (None, F9, false),
        ] {
            let held = run_button(&[(scope, NONE), (scope, key)]);
            assert_eq!(held, [false, counts], "{scope:?} {key:?}");
        }
    }

    #[test]
    fn system_button_follows_a_fresh_press_until_release() {
        use InputScope::*;
        assert_eq!(
            run_button(&[
                (SteamVr, NONE),
                (SteamVr, F9),
                (SteamVr, F9),
                (SteamVr, NONE),
                (SteamVr, F9),
            ]),
            [false, true, true, false, true]
        );
        // Leaving the window releases the button even with the key still down.
        assert_eq!(
            run_button(&[(App, NONE), (App, M), (None, M), (App, M)]),
            [false, true, false, false]
        );
    }

    #[test]
    fn system_button_needs_the_key_alone() {
        use InputScope::*;
        for scope in [SteamVr, App, Anywhere] {
            for key in [M, F9] {
                let held = run_button(&[(scope, NONE), (scope, with_modifier(key))]);
                assert_eq!(held, [false, false], "{scope:?} {key:?}");
            }
        }
        // A modifier pressed after the key does not release the button.
        assert_eq!(
            run_button(&[(App, NONE), (App, M), (App, with_modifier(M))]),
            [false, true, true]
        );
    }

    #[test]
    fn letting_go_of_the_modifier_first_does_not_press_the_button() {
        use InputScope::*;
        // Ctrl+F9, then Ctrl released while F9 is still down.
        assert_eq!(
            run_button(&[
                (SteamVr, NONE),
                (SteamVr, with_modifier(F9)),
                (SteamVr, F9),
                (SteamVr, NONE),
            ]),
            [false, false, false, false]
        );
    }

    #[test]
    fn a_key_held_while_moving_in_does_not_press_the_button() {
        use InputScope::*;
        // F9 held down in another window, then SteamVR's window comes to the front.
        assert_eq!(
            run_button(&[
                (None, NONE),
                (None, F9),
                (SteamVr, F9),
                (SteamVr, NONE),
                (SteamVr, F9)
            ]),
            [false, false, false, false, true]
        );
        // F9 held in the VR app, where it does not count, then SteamVR in front.
        assert_eq!(
            run_button(&[(App, NONE), (App, F9), (SteamVr, F9)]),
            [false, false, false]
        );
        // A key already down when the driver starts.
        assert_eq!(run_button(&[(App, M), (App, M)]), [false, false]);
    }

    #[test]
    fn system_keys_are_not_look_keys() {
        assert_eq!(key_look(keys(|k| k.system = true)), KeyLook::default());
        assert_eq!(
            key_look(keys(|k| k.system_steamvr = true)),
            KeyLook::default()
        );
    }

    fn display(width: i32, height: i32, scale: f32) -> DisplayConfig {
        DisplayConfig {
            window_width: width,
            window_height: height,
            horizontal_fov_deg: 100.0,
            render_scale: scale,
            display_frequency: 60.0,
            ipd_meters: 0.063,
            head_height_meters: 0.0,
        }
    }

    fn render_edges(d: &DisplayConfig) -> (f32, f32) {
        (
            d.window_width as f32 * d.render_scale,
            d.window_height as f32 * d.render_scale,
        )
    }

    #[test]
    fn display_defaults_pass_through_unchanged() {
        let d = display(1600, 900, 1.0);
        assert_eq!(d.sanitized(), d);
    }

    #[test]
    fn display_negative_zero_and_oversized_dimensions_are_clamped() {
        for (w, h) in [
            (-1, -1),
            (0, 0),
            (i32::MIN, 5),
            (i32::MAX, i32::MAX),
            (100_000, 900),
        ] {
            let d = display(w, h, 1.0).sanitized();
            assert!(
                (MIN_DIMENSION..=MAX_DIMENSION).contains(&d.window_width),
                "{w}x{h} -> {d:?}"
            );
            assert!(
                (MIN_DIMENSION..=MAX_DIMENSION).contains(&d.window_height),
                "{w}x{h} -> {d:?}"
            );
        }
    }

    #[test]
    fn display_render_target_never_exceeds_the_limit() {
        for scale in [0.0, 1.0, 4.0, 1e9, f32::INFINITY, f32::NAN, -3.0] {
            for (w, h) in [(1600, 900), (16384, 16384), (64, 64), (-1, 8000)] {
                let d = display(w, h, scale).sanitized();
                let (rw, rh) = render_edges(&d);
                assert!(
                    d.render_scale.is_finite() && d.render_scale >= 0.25,
                    "{d:?}"
                );
                assert!(
                    rw <= MAX_DIMENSION as f32 && rh <= MAX_DIMENSION as f32,
                    "{d:?}"
                );
                assert!(rw >= 16.0 && rh >= 16.0, "{d:?}");
            }
        }
    }

    #[test]
    fn display_non_finite_values_fall_back_to_defaults() {
        let mut d = display(1600, 900, f32::NAN);
        d.horizontal_fov_deg = f32::NAN;
        d.display_frequency = f32::INFINITY;
        d.ipd_meters = f32::NEG_INFINITY;
        d.head_height_meters = f32::NAN;
        assert_eq!(d.sanitized(), display(1600, 900, 1.0));
    }

    #[test]
    fn input_config_non_finite_gains_fall_back() {
        let cfg = Config {
            mouse_deg_per_px: f32::NAN,
            key_deg_per_sec: f32::INFINITY,
            grab_style: false,
            any_window: true,
            smoothing_ms: -5.0,
        }
        .sanitized();
        assert_eq!(cfg.mouse_deg_per_px, 0.12);
        assert_eq!(cfg.key_deg_per_sec, 60.0);
        assert_eq!(cfg.smoothing_ms, 0.0);
        assert!(!cfg.grab_style && cfg.any_window);
    }

    fn assert_close(a: [f64; 4], b: [f64; 4]) {
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() < 1e-12, "{a:?} != {b:?}");
        }
    }

    #[test]
    fn rotation_identity_and_quarter_turns() {
        let h = std::f64::consts::FRAC_1_SQRT_2;
        assert_close(rotation_from_yaw_pitch(0.0, 0.0), [1.0, 0.0, 0.0, 0.0]);
        // 90 degrees left: about +Y.
        assert_close(rotation_from_yaw_pitch(90.0, 0.0), [h, 0.0, h, 0.0]);
        // 90 degrees up: about +X.
        assert_close(rotation_from_yaw_pitch(0.0, 90.0), [h, h, 0.0, 0.0]);
    }

    #[test]
    fn rotation_is_a_unit_quaternion() {
        for (yaw, pitch) in [(37.0, -12.0), (-170.0, 89.0), (725.0, 0.5)] {
            let q = rotation_from_yaw_pitch(yaw, pitch);
            let norm: f64 = q.iter().map(|c| c * c).sum();
            assert!((norm - 1.0).abs() < 1e-12);
        }
    }
}
