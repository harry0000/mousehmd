# mousehmd

[English](README.en.md) | 日本語

マウスとキーボードで向きを動かせる、SteamVR 用の仮想 HMD ドライバ。
ゴーグルなしで VR アプリの視点を動かすためのもの。

## 動作環境

| 項目 | 要件 |
|---|---|
| OS | 64 ビットの Windows 10 以降 |
| SteamVR | 必須。動作を確認した版は 2.18.4 |
| VR アプリ | 既定の入力対象は DMM VR Player（`DMMVRPlayer_Windows.exe`）と SteamVR のウィンドウ。[設定](#設定)の `anyWindow` で全ウィンドウに変更できる |
| ランタイム | Visual C++ 再頒布可能パッケージ（`MSVCP140.dll`、`VCRUNTIME140.dll`）。SteamVR も同じものを使うので、SteamVR が動く PC には入っている |
| ビルドする場合 | Rust 1.89.0（MSVC ツールチェーン）、Visual Studio の C++ ビルドツール |

## インストール

1. SteamVR を終了する。
2. [Releases](https://github.com/harry0000/mousehmd/releases) から `mousehmd-vX.Y.Z.zip` をダウンロードする。
3. （任意）zip がこのリポジトリの GitHub Actions でビルドされたものか、[GitHub CLI](https://cli.github.com/) で確かめる。

   ```powershell
   gh attestation verify mousehmd-vX.Y.Z.zip --repo harry0000/mousehmd
   ```

4. 版を含まない名前のフォルダ（例: `C:\Tools\mousehmd`）に展開する。エクスプローラーの「すべて展開」は `mousehmd-vX.Y.Z` を提案するので、展開先を書き換える。
5. 展開した中の `mousehmd` フォルダ（`driver.vrdrivermanifest` のあるフォルダ）を、SteamVR 同梱の `vrpathreg.exe` で登録する。

   ```powershell
   & "C:\Program Files (x86)\Steam\steamapps\common\SteamVR\bin\win64\vrpathreg.exe" adddriver "C:\Tools\mousehmd\mousehmd"
   ```

   `vrpathreg.exe` を使う代わりに、`%LOCALAPPDATA%\openvr\openvrpaths.vrpath` の `external_drivers` に絶対パスを追加してもよい。
6. `Steam\config\steamvr.vrsettings` の `steamvr` に `"forcedDriver": "mousehmd"` と `"activateMultipleDrivers": true` を書く。

   ```json
   "steamvr" : {
      "activateMultipleDrivers" : true,
      "forcedDriver" : "mousehmd",
      ...
   }
   ```

   すでにある項目は、書き換える前の値を控えておく（アンインストールで戻すときに使う）。どちらも SteamVR 全体の設定で、ドライバを消しても残る。
7. SteamVR を起動する。DMM VR Player または SteamVR の表示ウィンドウを前面にして、右ボタンのドラッグで視点が動けば完了。
   動かない場合は、`Steam\logs\vrserver.txt` に `mousehmd: activated` が出ているか確かめる。
8. （任意）起動時の通知を消す（[起動時の「ヘッドセット通知」を消す](#起動時のヘッドセット通知を消す)）。

## 更新

1. SteamVR を終了する。
2. 展開先のフォルダの中身を消す。
3. 新しい zip を同じフォルダに展開する。登録し直す必要はない。

別のフォルダに展開した場合は、古いフォルダを `vrpathreg.exe removedriver` で外してから、新しいフォルダを `adddriver` で登録する。外さないと、SteamVR は古いフォルダの DLL を読み込み続ける。

ダッシュボードを無効にしていた以前の版（起動時に `dashboard.enableDashboard` を `false` に書き換えていた）から更新した場合は、SteamVR を終了した状態で `steamvr.vrsettings` の `dashboard.enableDashboard` を削除するか `true` にする。意図して無効にしている場合は、そのままでよい。

## 操作

対象ウィンドウが前面にあるときだけ反応する。

| 操作 | VR アプリ | SteamVR のウィンドウ | 動作 |
|---|---|---|---|
| 右ボタンを押しながらドラッグ | ○ | ○ | 見回す |
| 中ボタン | ○ | ○ | 正面に戻す |
| I / K / J / L | ○ | － | 上 / 下 / 左 / 右を見る |
| H | ○ | － | 正面に戻す |
| M | ○ | － | SteamVR のダッシュボードを開く / 閉じる |
| F9 | － | ○ | SteamVR のダッシュボードを開く / 閉じる |

- 対象は実行ファイル名で判定する（大文字小文字は区別しない）。一覧は `src/input.rs` の `APP_PROCESSES`（VR アプリ）と `STEAMVR_PROCESSES`（SteamVR のウィンドウ）。
- SteamVR のウィンドウでは F9 以外のキーを無視する。SteamVR 自身のキー操作と重なるため（表示ウィンドウで M を押すと Legacy Mirror が開く、など）。VR アプリを起動していなくても、SteamVR のウィンドウから F9 でダッシュボードを開閉できる。
- M と F9 はコントローラのシステムボタンの代わりで、SteamVR が予約している入力 `/input/system/click` として送る。コントローラがないと、開いたダッシュボードを閉じる手段はほかにない。入力を送るだけでは効かないので、同梱の割り当て（`resources/input/bindings/vrcompositor_bindings_mousehmd.json`）でこの入力を `ToggleDashboard` に結び付けている。
- キーは前面のアプリにもそのまま届く。Ctrl・Alt・Shift・Space・Enter・矢印キー・WASD は Unity の既定の入力割り当てなので避けている。
- Ctrl・Alt・Shift・Windows キーを押している間は、I / K / J / L / H / M / F9 を無視する（`Ctrl+H` などはアプリ側のショートカットとみなす）。マウス操作には影響しない。
- M と F9 は、対象のウィンドウで押し始めたときだけ受け付ける。`Ctrl+F9` から Ctrl だけを先に離した場合や、キーを押したまま別のウィンドウから切り替えた場合は、押し直すまで反応しない。

## 設定

`steamvr.vrsettings` に `driver_mousehmd` セクションを書いて上書きする。既定値は `driver/mousehmd/resources/settings/default.vrsettings`。
範囲外の値や数値として不正な値は、安全な範囲に丸めるか既定値に置き換える（ウィンドウの各辺は 64〜16384 ピクセル、`renderScale` は 0.25〜4 で、描画サイズが 16384 ピクセルを超えない範囲）。

例: 表示ウィンドウを 1280×720 にし、視点の動きを少しなめらかにする場合（`steamvr` と同じ階層に書く）。

```json
"driver_mousehmd" : {
   "windowWidth" : 1280,
   "windowHeight" : 720,
   "smoothingMs" : 30
},
```

| キー | 既定値 | 意味 |
|---|---|---|
| `windowWidth` / `windowHeight` | 主モニターの縦横それぞれ半分に収まる 16:9（4K なら 1920×1080。収まるなら 960×540 以上で、タスクバーを除いた範囲を超えない） | 表示ウィンドウの大きさ（実際のピクセル数）。片方だけ指定すると、もう片方は 16:9 になる。SteamVR は表示ウィンドウを主モニターの中央に置き、大きさは SteamVR を起動し直すまで変わらない |
| `monoView` | true | true で片目だけを全面表示、false で両眼を左右に並べる |
| `horizontalFovDeg` | 100 | 水平視野角（度） |
| `renderScale` | 1.0 | 描画解像度の倍率 |
| `mouseDegPerPx` | 0.12 | ドラッグ 1 ピクセルあたりの回転量（度） |
| `keyDegPerSec` | 60 | キー操作の回転速度（度/秒） |
| `smoothingMs` | 0 | 視点の動きをなめらかにするフィルタの時定数（ミリ秒）。0 で無効。大きいほどなめらかだが遅れる |
| `grabStyle` | true | true で映像をつかんで動かす向き、false で FPS と同じ向き |
| `anyWindow` | false | true でどのウィンドウが前面でも反応する（すべてのキーが効き、ダッシュボードは M と F9 のどちらでも開閉できる） |
| `headHeightMeters` | 0 | 頭の高さ |

## 起動時の「ヘッドセット通知」を消す

SteamVR は起動のたびに「ヘッドセットがモニターとして実行されていることを SteamVR が検出しました」という通知を出す。ウィンドウに表示する仮想 HMD では常にこの条件に当てはまる。動作には影響しない。

1. SteamVR を起動する。
2. `Win + R` の「ファイル名を指定して実行」で、次を実行する。

   ```
   vrmonitor://ignoredismissablewarning/DisplayInExtendedMode
   ```

SteamVR 自身の「警告を無視する」機能が呼び出され、`steamvr.vrsettings` に次の項目が書き込まれる。

```json
"DismissedWarnings": {
   "DismissableWarning_DisplayInExtendedMode": true
}
```

止まるのは表示だけで、SteamVR は引き続き同じ状態を検出している。実機の HMD をつないだときにも同じ警告が出なくなるので、戻したいときはこの項目を削除する。

## 副作用

| 対象 | 影響 |
|---|---|
| `steamvr.forcedDriver` | インストールの手順で SteamVR のドライバ選択を `mousehmd` に固定する。実機の HMD を使うときは戻す必要がある |
| キー入力 | 読んだキーは前面のアプリにも届くので、アプリが同じキーに操作を割り当てていると両方が動く。ほかのソフトが F9 を PC 全体のホットキーにしている場合も、SteamVR のウィンドウで押すと両方が動く。`anyWindow: true` にすると、どのウィンドウが前面でもマウスとキーの両方に反応する |
| 装着状態 | 仮想 HMD は常に「装着中」と報告する。ヘッドセットを外したときの待機や一時停止は働かない |
| 実行中の負荷 | 入力と姿勢通知のスレッドが約 2 ミリ秒間隔で動き、タイマー分解能 1 ミリ秒を要求する。CPU の起床回数と消費電力がわずかに増える |
| 出力ファイル | `dashboardwatch` は実行したフォルダの `dashboardwatch.log` を上書きする |

## アンインストール

1. SteamVR を終了する。`dashboardwatch` を実行中なら、それも終了する。
2. 登録したドライバフォルダを `vrpathreg.exe` で外す（`openvrpaths.vrpath` の `external_drivers` から削除してもよい）。

   ```powershell
   & "C:\Program Files (x86)\Steam\steamapps\common\SteamVR\bin\win64\vrpathreg.exe" removedriver "C:\Tools\mousehmd\mousehmd"
   ```

3. `Steam\config\steamvr.vrsettings` を導入前の状態に戻す。新しく追加した項目は削除し、書き換えた項目は控えておいた元の値に戻す。
   - `steamvr.forcedDriver` と `steamvr.activateMultipleDrivers`（インストールの手順 6 で書いたもの）
   - `dashboard.enableDashboard`（以前の版を使っていた場合に `false` が書かれている。元の値が分からず、ダッシュボードを使いたい場合は削除するか `true` にする）
   - `DismissedWarnings.DismissableWarning_DisplayInExtendedMode`（通知を消す手順を実行した場合）
   - `driver_mousehmd` のセクション（設定を上書きしていた場合）
4. 展開したフォルダ（ソースからビルドした場合は `driver\mousehmd\bin`）と、`dashboardwatch.log` を削除する。
5. SteamVR を起動し、通常の HMD が選ばれていること、ダッシュボードが意図した状態になっていることを確かめる。

## 開発

### 構成

- `src/` — Rust。入力のポーリングと向き（yaw / pitch）の管理（`input.rs`）、判定と設定値の検証（`logic.rs`）、DLL のエントリポイント（`lib.rs`）
- `cpp/shim.cpp` — OpenVR のドライバインターフェース（C++ の抽象クラス）の実装
- `cpp/openvr_driver.h` — Valve の OpenVR SDK ヘッダ（BSD-3-Clause、`cpp/OPENVR_LICENSE.txt`）
- `driver/mousehmd/` — SteamVR に登録するドライバフォルダ。`resources/input/` に、仮想 HMD の入力プロファイルと、システムボタンをダッシュボードの開閉に結び付ける割り当てを置いている
- `dist/` — リリース用の zip を作るスクリプトと、DLL に含まれる第三者のライセンス表示
- `tools/dashboardwatch/` — ダッシュボードの開閉を確かめるツール

### ビルドとテスト

1. SteamVR を終了する（起動中は DLL がロックされていてビルドが失敗する）。
2. ビルドする。`driver\mousehmd\bin\win64\driver_mousehmd.dll` ができる。

   ```powershell
   powershell -ExecutionPolicy Bypass -File .\build.ps1
   ```

3. ルートからテストする。

   ```powershell
   cargo test
   cargo test --manifest-path tools\dashboardwatch\Cargo.toml
   ```

4. [インストール](#インストール)の手順 5 以降を行い、`adddriver` の例のパスは、ビルドした `driver\mousehmd` フォルダの絶対パスに置き換える。

GitHub Actions（`.github/workflows/test.yml`）は、`main` への push と pull request のたびに、この 2 つのフォルダで `cargo fmt --check`・`cargo clippy --all-targets -- -D warnings`・`cargo test` を Windows 上で実行する。
GitHub Actions のアクションと Cargo の依存クレートは、Dependabot（`.github/dependabot.yml`）が週 1 回確かめ、新しい版が出て 7 日たったものを pull request で提案する。Rust のツールチェーン（`.github/actions/setup/action.yml`）は対象外なので、手で更新する。

### リリース

1. リリースする版を決め、必要なら `Cargo.toml` の `version` を更新する。版を変更した場合は `cargo check` で `Cargo.lock` も更新する。リリースに含む変更をコミットする。
2. そのコミットに `v` と `version` を合わせたタグ（例: `v0.1.0`）を付けて push する。GitHub Actions（`.github/workflows/release.yml`）がビルドした DLL を zip にまとめ、ビルドの出どころの証明（attestation）を付けて、下書きのリリースに添付する。
3. 下書きの内容を確かめてから、手で公開する。

同じ zip は手元でも作れる（`dist\out\` に出力される）。

```powershell
cargo build --release --locked
powershell -ExecutionPolicy Bypass -File .\dist\package.ps1 -Version v0.1.0
```

### 確認ツール

`tools/dashboardwatch` は、SteamVR のダッシュボードが表示中かどうかを SteamVR に問い合わせ、変化した時刻を `dashboardwatch.log` に 1 行ずつ書き出す（既定で 60 秒間）。キー操作でダッシュボードが開閉したかを、画面を見ずに確かめるためのもの。

```powershell
cd tools\dashboardwatch
cargo run --release
```

`cargo run --release -- --open 30` のように `--open` を付けると、最初にダッシュボードを開き、表示されたのを確かめてから 30 秒間の記録を始める。閉じる操作を毎回同じ条件で試すためのもの。開いたこと自体は変化の回数に数えない。5 秒以内に表示されなければ、記録を始めずにエラーで終わる（終了コードが 0 以外になり、理由が `dashboardwatch.log` に残る）。

## ライセンス

[MIT License](LICENSE)。
ただし `cpp/openvr_driver.h` は Valve の OpenVR SDK のファイルで、BSD-3-Clause（`cpp/OPENVR_LICENSE.txt`）に従う。
