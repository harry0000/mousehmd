# mousehmd

マウスとキーボードで向きを動かせる、SteamVR 用の仮想 HMD ドライバ。
ゴーグルなしで VR アプリの視点を動かすためのもの。

## 構成

- `src/` — Rust。入力のポーリングと向き（yaw / pitch）の管理（`input.rs`）、判定と設定値の検証（`logic.rs`）、DLL のエントリポイント（`lib.rs`）
- `cpp/shim.cpp` — OpenVR のドライバインターフェース（C++ の抽象クラス）の実装
- `cpp/openvr_driver.h` — Valve の OpenVR SDK ヘッダ（BSD-3-Clause、`cpp/OPENVR_LICENSE.txt`）
- `driver/mousehmd/` — SteamVR に登録するドライバフォルダ。`resources/input/` に、仮想 HMD の入力プロファイルと、システムボタンをダッシュボードの開閉に結び付ける割り当てを置いている

## ビルド

SteamVR を終了した状態で実行する。Rust（MSVC ツールチェーン）と Visual Studio の C++ ビルドツールが必要。

```powershell
powershell -ExecutionPolicy Bypass -File .\build.ps1
```

`driver\mousehmd\bin\win64\driver_mousehmd.dll` ができる。

テストは `cargo test` で実行する。

## SteamVR への登録

1. `%LOCALAPPDATA%\openvr\openvrpaths.vrpath` の `external_drivers` に `driver\mousehmd` の絶対パスを追加
2. `Steam\config\steamvr.vrsettings` の `steamvr.forcedDriver` を `"mousehmd"` にする

### 以前の版から更新する場合

以前の版は、既定でダッシュボードを無効にしていた（起動時に `steamvr.vrsettings` の `dashboard.enableDashboard` を `false` に書き換えていた）。
今の版は SteamVR のこの設定に触らないが、すでに書き込まれた値は更新しても自動では戻らない。ダッシュボードを使う（M で開閉する）場合は、SteamVR を終了した状態で `dashboard.enableDashboard` を削除するか `true` にする。
意図して無効にしている場合は、そのままでよい。

## 操作

対象ウィンドウが前面にあるときだけ反応する。マウスは VR アプリと SteamVR のウィンドウの両方で、キーは VR アプリのウィンドウだけで効く。

| 操作 | 動作 |
|---|---|
| 右ボタンを押しながらドラッグ | 見回す |
| 中ボタン | 正面に戻す |
| I / K / J / L | 上 / 下 / 左 / 右を見る |
| H | 正面に戻す |
| M | SteamVR のダッシュボードを開く / 閉じる |

対象は実行ファイル名で判定する（大文字小文字は区別しない）。一覧は `src/input.rs` の `APP_PROCESSES`（VR アプリ）と `STEAMVR_PROCESSES`（SteamVR のウィンドウ）。
SteamVR のウィンドウでキーを無視するのは、SteamVR 自身がキー操作を持っているため（表示ウィンドウで M を押すと Legacy Mirror が開く、など）。

M はコントローラのシステムボタンの代わりで、SteamVR が予約している入力 `/input/system/click` として送る。
コントローラがないと、開いたダッシュボードを閉じる手段がほかにない。
入力を送るだけでは効かず、同梱の割り当て（`resources/input/bindings/vrcompositor_bindings_mousehmd.json`）がこの入力を `ToggleDashboard` に結び付けている。

キーは前面のアプリにもそのまま届く。Ctrl・Alt・Shift・Space・Enter・矢印キー・WASD は Unity の既定の入力割り当てなので避けている。
Ctrl・Alt・Shift・Windows キーを押している間は、I / K / J / L / H / M を無視する（`Ctrl+H` などはアプリ側のショートカットとみなす）。マウス操作には影響しない。

## 設定

既定値は `driver/mousehmd/resources/settings/default.vrsettings`。
範囲外の値や数値として不正な値は、安全な範囲に丸めるか既定値に置き換える（ウィンドウの各辺は 64〜16384 ピクセル、`renderScale` は 0.25〜4 で、描画サイズが 16384 ピクセルを超えない範囲）。
変えるときは `steamvr.vrsettings` に `driver_mousehmd` セクションを書いて上書きする。

| キー | 既定値 | 意味 |
|---|---|---|
| `windowX` / `windowY` | 100 | 表示ウィンドウの位置 |
| `windowWidth` / `windowHeight` | 1600 / 900 | 表示ウィンドウの大きさ |
| `monoView` | true | true で片目だけを全面表示、false で両眼を左右に並べる |
| `horizontalFovDeg` | 100 | 水平視野角（度） |
| `renderScale` | 1.0 | 描画解像度の倍率 |
| `mouseDegPerPx` | 0.12 | ドラッグ 1 ピクセルあたりの回転量（度） |
| `keyDegPerSec` | 60 | キー操作の回転速度（度/秒） |
| `smoothingMs` | 0 | 視点の動きをなめらかにするフィルタの時定数（ミリ秒）。0 で無効。大きいほどなめらかだが遅れる |
| `grabStyle` | true | true で映像をつかんで動かす向き、false で FPS と同じ向き |
| `anyWindow` | false | true でどのウィンドウが前面でも反応する |
| `headHeightMeters` | 0 | 頭の高さ |

## 起動時の「ヘッドセット通知」を消す

SteamVR は起動のたびに「ヘッドセットがモニターとして実行されていることを SteamVR が検出しました」という通知を出す。
ウィンドウに表示する仮想 HMD では常にこの条件に当てはまる。動作には影響しない。

SteamVR を起動した状態で、`Win + R` の「ファイル名を指定して実行」から次を実行すると、以後は表示されなくなる。

```
vrmonitor://ignoredismissablewarning/DisplayInExtendedMode
```

これは SteamVR 自身の「警告を無視する」機能を呼び出すもので、`steamvr.vrsettings` に次の項目が書き込まれる。

```json
"DismissedWarnings": {
   "DismissableWarning_DisplayInExtendedMode": true
}
```

止まるのは表示だけで、SteamVR は引き続き同じ状態を検出している。
実機の HMD をつないだときにも同じ警告が出なくなるので、戻したいときはこの項目を削除する。

## ライセンス

[MIT License](LICENSE)。
ただし `cpp/openvr_driver.h` は Valve の OpenVR SDK のファイルで、BSD-3-Clause（`cpp/OPENVR_LICENSE.txt`）に従う。
