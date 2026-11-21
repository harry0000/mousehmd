# mousehmd

English | [日本語](README.md)

A virtual HMD driver for SteamVR whose orientation is driven by the mouse and keyboard.
It lets you look around in VR apps without a headset.

## Layout

- `src/` — Rust. Input polling and orientation (yaw / pitch) (`input.rs`), decisions and validation of settings (`logic.rs`), and the DLL entry points (`lib.rs`)
- `cpp/shim.cpp` — Implementation of the OpenVR driver interfaces (C++ abstract classes)
- `cpp/openvr_driver.h` — Header from Valve's OpenVR SDK (BSD-3-Clause, `cpp/OPENVR_LICENSE.txt`)
- `driver/mousehmd/` — The driver folder registered with SteamVR. `resources/input/` holds the virtual HMD's input profile and the binding that ties the system button to opening and closing the dashboard
- `dist/` — The script that makes the release zip, and the notices of the third-party code in the DLL

## Getting it

Download `mousehmd-vX.Y.Z.zip` from [Releases](https://github.com/harry0000/mousehmd/releases) and extract it to a place where it will stay (for example `C:\Tools\mousehmd`).
Keep the version out of the folder name and extract new versions to the same folder. Explorer's "Extract All" suggests a folder named after the zip (`mousehmd-vX.Y.Z`), so change the destination.
Register the `mousehmd` folder inside it as described in "Registering with SteamVR" below. No build is needed.
It needs 64-bit Windows 10 or later and SteamVR.

To update, close SteamVR, delete the contents of the folder, and extract the new zip to the same folder. There is no need to register it again.
If you extract it to a different folder, unregister the old folder before registering the new one. Otherwise SteamVR keeps loading the DLL from the old folder.

You can check with the [GitHub CLI](https://cli.github.com/) that the zip was built by this repository's GitHub Actions.

```powershell
gh attestation verify mousehmd-vX.Y.Z.zip --repo harry0000/mousehmd
```

To build from source instead, register the `driver\mousehmd` folder made in "Building" below.

## Building

Run with SteamVR closed. Requires Rust (MSVC toolchain) and the Visual Studio C++ build tools.

```powershell
powershell -ExecutionPolicy Bypass -File .\build.ps1
```

This produces `driver\mousehmd\bin\win64\driver_mousehmd.dll`.

Run the tests with `cargo test` (run them separately in `tools\dashboardwatch` as well).
GitHub Actions (`.github/workflows/test.yml`) runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` in both folders on Windows for every push to `main` and every pull request.
Dependabot (`.github/dependabot.yml`) checks the GitHub Actions and the Cargo dependencies once a week, and proposes a new version in a pull request once it has been out for 7 days. The Rust toolchain (`.github/actions/setup/action.yml`) is not covered and is updated by hand.

Releases are made by GitHub Actions (`.github/workflows/release.yml`) when a tag named after the `version` in `Cargo.toml` (for example `v0.1.0`) is pushed. It packs the built DLL into a zip, attaches a build provenance attestation, and adds the zip to a draft release. Check the draft, then publish it by hand.
The same zip can be made locally (it is written to `dist\out\`).

```powershell
cargo build --release --locked
powershell -ExecutionPolicy Bypass -File .\dist\package.ps1 -Version v0.1.0
```

## Registering with SteamVR

Do this with SteamVR closed.

1. Register the driver, either with `vrpathreg.exe`, which comes with SteamVR (in `SteamVR\bin\win64`), or by adding the absolute path of the driver folder to `external_drivers` in `%LOCALAPPDATA%\openvr\openvrpaths.vrpath`. The driver folder is the `mousehmd` folder inside the extracted zip, or `driver\mousehmd` when built from source.

   ```powershell
   & "C:\Program Files (x86)\Steam\steamapps\common\SteamVR\bin\win64\vrpathreg.exe" adddriver "C:\Tools\mousehmd\mousehmd"
   ```

   Give the absolute path of the folder that contains `driver.vrdrivermanifest`, not of the DLL.
2. In `Steam\config\steamvr.vrsettings`, add `"forcedDriver": "mousehmd"` and `"activateMultipleDrivers": true` to `steamvr`.

Both are settings of SteamVR as a whole and stay after the driver is removed.
If either of the two keys written in step 2 already exists, note its current value before changing it. You will need it to restore the setting when uninstalling.

### Updating from an earlier version

Earlier versions disabled the dashboard by default (on startup they set `dashboard.enableDashboard` in `steamvr.vrsettings` to `false`).
The current version leaves this setting alone, but a value already written is not restored by updating. To use the dashboard (opened and closed with M), close SteamVR and then delete `dashboard.enableDashboard` or set it to `true`.
If you disabled it on purpose, leave it as it is.

## Controls

The driver only reacts while one of its target windows is in front. The mouse works in both the VR app and SteamVR's windows. The keys work in the VR app's window; in SteamVR's windows only F9 works.

| Input | VR app | SteamVR windows | Action |
|---|---|---|---|
| Drag with the right button | ✓ | ✓ | Look around |
| Middle button | ✓ | ✓ | Recenter |
| I / K / J / L | ✓ | – | Look up / down / left / right |
| H | ✓ | – | Recenter |
| M | ✓ | – | Open / close the SteamVR dashboard |
| F9 | – | ✓ | Open / close the SteamVR dashboard |

Windows are matched by executable file name (case-insensitive). The lists are `APP_PROCESSES` (VR apps) and `STEAMVR_PROCESSES` (SteamVR's windows) in `src/input.rs`.
Keys other than F9 are ignored in SteamVR's windows because SteamVR has keyboard shortcuts of its own (pressing M in the display window opens the Legacy Mirror, for example).
For that reason the dashboard key there is F9, which SteamVR does not use in its windows. The dashboard can be opened and closed from SteamVR's windows even with no VR app running.

M and F9 stand in for a controller's system button and are sent as `/input/system/click`, the input SteamVR reserves for it.
Without a controller there is no other way to close a dashboard once it is open.
Sending the input alone does not work: the bundled binding (`resources/input/bindings/vrcompositor_bindings_mousehmd.json`) maps it to `ToggleDashboard`.

The keys also reach the app in front. Ctrl, Alt, Shift, Space, Enter, the arrow keys and WASD are avoided because they are Unity's default input bindings.
While Ctrl, Alt, Shift or a Windows key is held, I / K / J / L / H / M / F9 are ignored (`Ctrl+H` and the like are taken as the app's own shortcuts). The mouse is not affected.
M and F9 only count when pressed in a target window. If you release Ctrl first after `Ctrl+F9`, or switch windows with the key held down, nothing happens until you press the key again.

## Settings

The defaults are in `driver/mousehmd/resources/settings/default.vrsettings`.
Values out of range or not valid numbers are clamped to a safe range or replaced by the default (each window edge 64–16384 pixels, `renderScale` 0.25–4, limited so that the render size stays within 16384 pixels).
To change them, override them in a `driver_mousehmd` section in `steamvr.vrsettings`.

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

On every start, SteamVR shows the notification "SteamVR has detected your headset running as a monitor."
A virtual HMD shown in a window always meets that condition. It does not affect how the driver works.

With SteamVR running, run the following from the Run dialog (`Win + R`), and the notification is no longer shown.

```
vrmonitor://ignoredismissablewarning/DisplayInExtendedMode
```

This calls SteamVR's own "ignore this warning" feature, which writes the following to `steamvr.vrsettings`.

```json
"DismissedWarnings": {
   "DismissableWarning_DisplayInExtendedMode": true
}
```

Only the notification stops; SteamVR still detects the same condition.
The same warning is also hidden when a real HMD is connected, so delete this entry to bring it back.

## Checking tool

`tools/dashboardwatch` asks SteamVR whether the dashboard is showing and writes each change, with its time, to `dashboardwatch.log`, one line at a time (for 60 seconds by default). It is for checking that a key press opened or closed the dashboard without watching the screen.

```powershell
cd tools\dashboardwatch
cargo run --release
```

With `--open`, as in `cargo run --release -- --open 30`, the tool first opens the dashboard and, once it has seen it showing, records for 30 seconds. This is for trying the close under the same conditions every time. The opening itself is not counted as a change. If the dashboard does not show within 5 seconds, the tool stops with an error without recording (the exit code is non-zero and the reason is left in `dashboardwatch.log`).

## Side effects

| What | Effect |
|---|---|
| `steamvr.forcedDriver` | The registration steps fix SteamVR's driver choice to `mousehmd`. Change it back to use a real HMD |
| Key input | The keys the driver reads also reach the app in front, so if the app uses the same key, both act on it. Likewise, if another program uses F9 as a system-wide hotkey, pressing it in SteamVR's windows triggers both. With `anyWindow: true`, both the mouse and the keys work whatever window is in front |
| Worn state | The virtual HMD always reports that it is being worn. Standby or pausing when the headset is taken off does not happen |
| Load while running | The input and pose threads run about every 2 milliseconds and request a 1 millisecond timer resolution. This slightly increases CPU wake-ups and power use |
| Output files | `dashboardwatch` overwrites `dashboardwatch.log` in the folder it is run from |

## Uninstalling

1. Close SteamVR. If `dashboardwatch` is running, stop it too.
2. Unregister the driver, either with `vrpathreg.exe` or by deleting the path of the driver folder you registered from `external_drivers` in `%LOCALAPPDATA%\openvr\openvrpaths.vrpath`.

   ```powershell
   & "C:\Program Files (x86)\Steam\steamapps\common\SteamVR\bin\win64\vrpathreg.exe" removedriver "C:\Tools\mousehmd\mousehmd"
   ```

   Give the same folder as when registering, the one that contains `driver.vrdrivermanifest`.
3. Restore `Steam\config\steamvr.vrsettings` to how it was before installation: delete the keys you added, and set the keys you changed back to the values you noted. These are:
   - `steamvr.forcedDriver` and `steamvr.activateMultipleDrivers` (written in the registration steps)
   - `dashboard.enableDashboard` (set to `false` if you used an earlier version. If you do not know the original value and want to use the dashboard, delete it or set it to `true`)
   - `DismissedWarnings.DismissableWarning_DisplayInExtendedMode` (if you followed the steps to hide the notification)
   - The `driver_mousehmd` section (if you overrode settings)
4. If no longer needed, delete the extracted folder (`driver\mousehmd\bin` if you built from source) and the tool's output (`dashboardwatch.log`).

Restart SteamVR and check that your usual HMD is selected and that the dashboard is in the state you intend.

## License

[MIT License](LICENSE).
`cpp/openvr_driver.h`, a file from Valve's OpenVR SDK, is under the BSD-3-Clause license instead (`cpp/OPENVR_LICENSE.txt`).
