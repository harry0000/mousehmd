# mousehmd

English | [日本語](README.md)

A virtual HMD driver for SteamVR whose orientation is driven by the mouse and keyboard.
It lets you look around in VR apps without a headset.

## Requirements

| Item | Requirement |
|---|---|
| OS | 64-bit Windows 10 or later |
| SteamVR | Required. Tested with 2.18.4 |
| VR app | Input is enabled in DMM VR Player (`DMMVRPlayer_Windows.exe`) and SteamVR windows by default. `anyWindow` in [Settings](#settings) enables input in all windows |
| Runtime | Visual C++ Redistributable (`MSVCP140.dll`, `VCRUNTIME140.dll`). SteamVR uses the same one, so it is installed wherever SteamVR runs |
| To build | Rust 1.89.0 (MSVC toolchain) and the Visual Studio C++ build tools |

## Installing

1. Close SteamVR.
2. Download `mousehmd-vX.Y.Z.zip` from [Releases](https://github.com/harry0000/mousehmd/releases).
3. (Optional) Check with the [GitHub CLI](https://cli.github.com/) that the zip was built by this repository's GitHub Actions.

   ```powershell
   gh attestation verify mousehmd-vX.Y.Z.zip --repo harry0000/mousehmd
   ```

4. Extract it to a folder without the version in its name (for example `C:\Tools\mousehmd`). Explorer's "Extract All" suggests `mousehmd-vX.Y.Z`, so change the destination.
5. Register the `mousehmd` folder inside it (the folder that contains `driver.vrdrivermanifest`) with `vrpathreg.exe`, which comes with SteamVR.

   ```powershell
   & "C:\Program Files (x86)\Steam\steamapps\common\SteamVR\bin\win64\vrpathreg.exe" adddriver "C:\Tools\mousehmd\mousehmd"
   ```

   Instead of `vrpathreg.exe`, you can add the absolute path to `external_drivers` in `%LOCALAPPDATA%\openvr\openvrpaths.vrpath`.
6. In `Steam\config\steamvr.vrsettings`, add `"forcedDriver": "mousehmd"` and `"activateMultipleDrivers": true` to `steamvr`.
   If either already exists, note its value before changing it (you need it to restore the setting when uninstalling). Both are settings of SteamVR as a whole and stay after the driver is removed.
7. Start SteamVR. Bring DMM VR Player or SteamVR's display window to the front. If dragging with the right button turns the view, you are done.
   If not, check that `Steam\logs\vrserver.txt` contains `mousehmd: activated`.
8. (Optional) Hide the notification shown on startup ([Hiding the headset notification on startup](#hiding-the-headset-notification-on-startup)).

## Updating

1. Close SteamVR.
2. Delete the contents of the folder you extracted to.
3. Extract the new zip to the same folder. There is no need to register it again.

If you extract it to a different folder, remove the old folder with `vrpathreg.exe removedriver` before registering the new one with `adddriver`. Otherwise SteamVR keeps loading the DLL from the old folder.

If you are updating from an earlier version that disabled the dashboard (it set `dashboard.enableDashboard` to `false` on startup), close SteamVR and delete `dashboard.enableDashboard` in `steamvr.vrsettings` or set it to `true`. If you disabled it on purpose, leave it as it is.

## Controls

The driver only reacts while one of its target windows is in front.

| Input | VR app | SteamVR windows | Action |
|---|---|---|---|
| Drag with the right button | ✓ | ✓ | Look around |
| Middle button | ✓ | ✓ | Recenter |
| I / K / J / L | ✓ | – | Look up / down / left / right |
| H | ✓ | – | Recenter |
| M | ✓ | – | Open / close the SteamVR dashboard |
| F9 | – | ✓ | Open / close the SteamVR dashboard |

- Windows are matched by executable file name (case-insensitive). The lists are `APP_PROCESSES` (VR apps) and `STEAMVR_PROCESSES` (SteamVR's windows) in `src/input.rs`.
- Keys other than F9 are ignored in SteamVR's windows, because they would clash with SteamVR's own shortcuts (pressing M in the display window opens the Legacy Mirror, for example). F9 opens and closes the dashboard from SteamVR's windows even with no VR app running.
- M and F9 stand in for a controller's system button and are sent as `/input/system/click`, the input SteamVR reserves for it. Without a controller there is no other way to close a dashboard once it is open. Sending the input alone does not work, so the bundled binding (`resources/input/bindings/vrcompositor_bindings_mousehmd.json`) maps it to `ToggleDashboard`.
- The keys also reach the app in front. Ctrl, Alt, Shift, Space, Enter, the arrow keys and WASD are avoided because they are Unity's default input bindings.
- While Ctrl, Alt, Shift or a Windows key is held, I / K / J / L / H / M / F9 are ignored (`Ctrl+H` and the like are taken as the app's own shortcuts). The mouse is not affected.
- M and F9 only count when pressed in a target window. If you release Ctrl first after `Ctrl+F9`, or switch windows with the key held down, nothing happens until you press the key again.

## Settings

Override them in a `driver_mousehmd` section in `steamvr.vrsettings`. The defaults are in `driver/mousehmd/resources/settings/default.vrsettings`.
Values out of range or not valid numbers are clamped to a safe range or replaced by the default (each window edge 64–16384 pixels, `renderScale` 0.25–4, limited so that the render size stays within 16384 pixels).

| Key | Default | Meaning |
|---|---|---|
| `windowX` / `windowY` | 100 | Position of the display window |
| `windowWidth` / `windowHeight` | 1600 / 900 | Size of the display window |
| `monoView` | true | true shows one eye over the whole window; false shows both eyes side by side |
| `horizontalFovDeg` | 100 | Horizontal field of view (degrees) |
| `renderScale` | 1.0 | Render resolution multiplier |
| `mouseDegPerPx` | 0.12 | Rotation per pixel of drag (degrees) |
| `keyDegPerSec` | 60 | Rotation speed of the look keys (degrees per second) |
| `smoothingMs` | 0 | Time constant of the filter that smooths the view's movement (milliseconds). 0 disables it. Larger values are smoother but lag more |
| `grabStyle` | true | true: dragging moves the picture as if grabbing it; false: same direction as an FPS |
| `anyWindow` | false | true reacts whatever window is in front (all keys work, and either M or F9 opens and closes the dashboard) |
| `headHeightMeters` | 0 | Head height |

## Hiding the headset notification on startup

On every start, SteamVR shows the notification "SteamVR has detected your headset running as a monitor." A virtual HMD shown in a window always meets that condition. It does not affect how the driver works.

1. Start SteamVR.
2. Run the following from the Run dialog (`Win + R`).

   ```
   vrmonitor://ignoredismissablewarning/DisplayInExtendedMode
   ```

This calls SteamVR's own "ignore this warning" feature, which writes the following to `steamvr.vrsettings`.

```json
"DismissedWarnings": {
   "DismissableWarning_DisplayInExtendedMode": true
}
```

Only the notification stops; SteamVR still detects the same condition. The same warning is also hidden when a real HMD is connected, so delete this entry to bring it back.

## Side effects

| What | Effect |
|---|---|
| `steamvr.forcedDriver` | The installation steps fix SteamVR's driver choice to `mousehmd`. Change it back to use a real HMD |
| Key input | The keys the driver reads also reach the app in front, so if the app uses the same key, both act on it. Likewise, if another program uses F9 as a system-wide hotkey, pressing it in SteamVR's windows triggers both. With `anyWindow: true`, both the mouse and the keys work whatever window is in front |
| Worn state | The virtual HMD always reports that it is being worn. Standby or pausing when the headset is taken off does not happen |
| Load while running | The input and pose threads run about every 2 milliseconds and request a 1 millisecond timer resolution. This slightly increases CPU wake-ups and power use |
| Output files | `dashboardwatch` overwrites `dashboardwatch.log` in the folder it is run from |

## Uninstalling

1. Close SteamVR. If `dashboardwatch` is running, stop it too.
2. Unregister the driver folder you registered with `vrpathreg.exe` (or delete it from `external_drivers` in `openvrpaths.vrpath`).

   ```powershell
   & "C:\Program Files (x86)\Steam\steamapps\common\SteamVR\bin\win64\vrpathreg.exe" removedriver "C:\Tools\mousehmd\mousehmd"
   ```

3. Restore `Steam\config\steamvr.vrsettings` to how it was before installation: delete the keys you added, and set the keys you changed back to the values you noted.
   - `steamvr.forcedDriver` and `steamvr.activateMultipleDrivers` (written in step 6 of the installation)
   - `dashboard.enableDashboard` (set to `false` if you used an earlier version. If you do not know the original value and want to use the dashboard, delete it or set it to `true`)
   - `DismissedWarnings.DismissableWarning_DisplayInExtendedMode` (if you followed the steps to hide the notification)
   - The `driver_mousehmd` section (if you overrode settings)
4. Delete the extracted folder (`driver\mousehmd\bin` if you built from source) and `dashboardwatch.log`.
5. Start SteamVR and check that your usual HMD is selected and that the dashboard is in the state you intend.

## Development

### Layout

- `src/` — Rust. Input polling and orientation (yaw / pitch) (`input.rs`), decisions and validation of settings (`logic.rs`), and the DLL entry points (`lib.rs`)
- `cpp/shim.cpp` — Implementation of the OpenVR driver interfaces (C++ abstract classes)
- `cpp/openvr_driver.h` — Header from Valve's OpenVR SDK (BSD-3-Clause, `cpp/OPENVR_LICENSE.txt`)
- `driver/mousehmd/` — The driver folder registered with SteamVR. `resources/input/` holds the virtual HMD's input profile and the binding that ties the system button to opening and closing the dashboard
- `dist/` — The script that makes the release zip, and the notices of the third-party code in the DLL
- `tools/dashboardwatch/` — A tool for checking that the dashboard opens and closes

### Building and testing

1. Close SteamVR (while it runs, the DLL is locked and the build fails).
2. Build. This produces `driver\mousehmd\bin\win64\driver_mousehmd.dll`.

   ```powershell
   powershell -ExecutionPolicy Bypass -File .\build.ps1
   ```

3. Run the tests from the repository root.

   ```powershell
   cargo test
   cargo test --manifest-path tools\dashboardwatch\Cargo.toml
   ```

4. Follow step 5 onwards of [Installing](#installing), replacing the example `adddriver` path with the absolute path to the built `driver\mousehmd` folder.

GitHub Actions (`.github/workflows/test.yml`) runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` in both folders on Windows for every push to `main` and every pull request.
Dependabot (`.github/dependabot.yml`) checks the GitHub Actions and the Cargo dependencies once a week, and proposes a new version in a pull request once it has been out for 7 days. The Rust toolchain (`.github/actions/setup/action.yml`) is not covered and is updated by hand.

### Releasing

1. Choose the release version and update `version` in `Cargo.toml` if needed. If the version changes, run `cargo check` to update `Cargo.lock` too. Commit the changes to include in the release.
2. Tag that commit with `v` plus the `version` (for example `v0.1.0`) and push it. GitHub Actions (`.github/workflows/release.yml`) packs the built DLL into a zip, attaches a build provenance attestation, and adds the zip to a draft release.
3. Check the draft, then publish it by hand.

The same zip can be made locally (it is written to `dist\out\`).

```powershell
cargo build --release --locked
powershell -ExecutionPolicy Bypass -File .\dist\package.ps1 -Version v0.1.0
```

### Checking tool

`tools/dashboardwatch` asks SteamVR whether the dashboard is showing and writes each change, with its time, to `dashboardwatch.log`, one line at a time (for 60 seconds by default). It is for checking that a key press opened or closed the dashboard without watching the screen.

```powershell
cd tools\dashboardwatch
cargo run --release
```

With `--open`, as in `cargo run --release -- --open 30`, the tool first opens the dashboard and, once it has seen it showing, records for 30 seconds. This is for trying the close under the same conditions every time. The opening itself is not counted as a change. If the dashboard does not show within 5 seconds, the tool stops with an error without recording (the exit code is non-zero and the reason is left in `dashboardwatch.log`).

## License

[MIT License](LICENSE).
`cpp/openvr_driver.h`, a file from Valve's OpenVR SDK, is under the BSD-3-Clause license instead (`cpp/OPENVR_LICENSE.txt`).
