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

/// How much of the input the window in the foreground should receive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputScope {
    /// Not one of ours: ignore everything.
    None,
    /// One of SteamVR's own windows. They have keyboard shortcuts of their own
    /// (M toggles the legacy mirror, for one), so only the mouse counts here.
    MouseOnly,
    /// The VR app: mouse and keys.
    MouseAndKeys,
}

/// Decides the scope from the foreground window's executable path.
pub fn input_scope(
    process_path: &str,
    app_targets: &[&str],
    steamvr_targets: &[&str],
) -> InputScope {
    if is_target_executable(process_path, app_targets) {
        InputScope::MouseAndKeys
    } else if is_target_executable(process_path, steamvr_targets) {
        InputScope::MouseOnly
    } else {
        InputScope::None
    }
}

/// Which keys are down right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    pub look_up: bool,
    pub look_down: bool,
    pub look_left: bool,
    pub look_right: bool,
    pub recenter: bool,
    /// The key standing in for a controller's system button.
    pub system: bool,
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

/// Whether the stand-in system button counts as held. Like the look keys, it
/// only counts when pressed on its own.
pub fn system_button(keys: Keys) -> bool {
    keys.system && !keys.modifier
}

/// Head rotation as a quaternion `[w, x, y, z]` in OpenVR's coordinate system
/// (+Y up, -Z forward): yaw about +Y, then pitch about the rotated +X.
/// Positive yaw turns left, positive pitch looks up.
pub fn rotation_from_yaw_pitch(yaw_deg: f64, pitch_deg: f64) -> [f64; 4] {
    let (sy, cy) = (yaw_deg.to_radians() / 2.0).sin_cos();
    let (sp, cp) = (pitch_deg.to_radians() / 2.0).sin_cos();
    [cy * cp, cy * sp, sy * cp, -sy * sp]
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn scope_gives_keys_to_the_app_and_only_the_mouse_to_steamvr() {
        let app = &["dmmvrplayer_windows.exe"];
        let steamvr = &["vrcompositor.exe", "vrmonitor.exe"];
        let scope = |path| input_scope(path, app, steamvr);
        assert_eq!(
            scope(r"C:\Apps\DMMVRPlayer_Windows.exe"),
            InputScope::MouseAndKeys
        );
        assert_eq!(
            scope(r"C:\SteamVR\bin\win64\vrcompositor.exe"),
            InputScope::MouseOnly
        );
        assert_eq!(
            scope(r"C:\SteamVR\bin\win64\vrmonitor.exe"),
            InputScope::MouseOnly
        );
        assert_eq!(scope(r"C:\Windows\notepad.exe"), InputScope::None);
        assert_eq!(scope(r"C:\vrcompositor.exe\notepad.exe"), InputScope::None);
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

    #[test]
    fn system_button_needs_the_key_alone() {
        assert!(system_button(keys(|k| k.system = true)));
        assert!(!system_button(Keys::default()));
        let ctrl_m = keys(|k| {
            k.system = true;
            k.modifier = true;
        });
        assert!(!system_button(ctrl_m));
        // It is not a look key.
        assert_eq!(key_look(keys(|k| k.system = true)), KeyLook::default());
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
