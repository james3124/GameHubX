# Reference salvage notes

Source: GameHub Lite 5.1.8 decompiled (`reference/gamehub-lite-v518-decompiled/`,
package `gamehub.lite`, versionCode 65, minSdk 26, targetSdk 34). Local-only,
gitignored. Everything here is reference for behavior and layout; no GameHub
code, binaries, art, fonts, or CDN endpoints are redistributed.

## Inventory

| Area | Location | Use |
|---|---|---|
| Manifest | `resources/AndroidManifest.xml` | permissions, landscape lock, activity map, gamepad feature |
| Layouts | `resources/res/layout/` | screen structure: `activity_pc_game_setting`, `activity_win_emu_setting`, `activity_settings`, `game_activity_game_library_main`, `llauncher_activity_main_guide_{1..4}` |
| Theme values | `resources/res/values*` | colors/dimens/styles to sample for parity |
| Fonts | `resources/res/font/` | `d_din_pro_600_semibold.otf` (commercial — do not ship), `roboto_medium_numbers.ttf` (Apache-2.0) |
| Touch profiles | `resources/assets/inputcontrols/profiles/*.icp` | normalized touch/gamepad mapping format (4 profiles) |
| Java sources | `sources/` | jadx output; flow reference only (import → container → launch) |
| Smali/resources | `apktool/` | raw resource inspection |
| Native libs | `apktool/lib/arm64-v8a/` | do not extract or reuse |

## Behavior worth mirroring

- Import flow: pick `.exe` → name/cover dialog → library entry → per-game
  settings → launch.
- Per-game settings surface: startup path, language, resolution, MangoHUD,
  window manager, env vars, command line, compatibility layer, translation
  parameters, DInput, GPU driver, surface format, audio driver.
- Dashboard: PC Emulator/Steam tiles, import CTA, drawer with Home/Game/
  Download/Setting, search, downloads and storage management.
- Controller vocabulary: A/B, LB/RB, Y, menu; landscape-only.

## Sibling project reuse

`/home/Project/GameHub/gamelite` (user-owned) supplies the proven scaffolding:
Android Gradle + NativeActivity, build-only CI, SQLite library, icon cache,
SHA-256 component installer, prefix/env planning, PSS diagnostics, 84 MB PSS
baseline. Ported selectively; the UI is eframe/egui, not Slint.

`/home/Project/gamelite` (native APK/ARSC/AXML/DEX toolchain) is a dev oracle
for inspecting the reference; it is not part of the launcher.

## Licensing

- Do not ship GameHub `libwinemu.so`, `libxserver.so`, other native libs, art,
  or CDN URLs.
- Runtime is rebuilt from open components: Wine (LGPL), Box64 (MIT), DXVK
  (zlib), Turnip/Mesa (MIT), X-server forks (MIT/X11).
- Replace DIN Pro with an OFL font; keep Roboto's Apache-2.0 notice if used.

## Batch 4 findings — P1 library and permissions (2026-11-16)

- Room table `t_game_library` (`GameLibraryTable`): `id`, `user_id`,
  `launch_type`, `package_name`, `game_id`, `local_game_id`,
  `local_mobile_app_id`, `data` (JSON), `time`, `is_last_launched`,
  `game_type`. `GameLibraryDao` provides insert-or-update, find-all by user,
  find-by-package/launch-type, delete, and last-launched tracking ordered by
  time. The PC catalog itself is server-backed; the local row carries IDs and
  a JSON blob, so our own SQLite schema (local ID, label, intro, exe path,
  favorite, last played) is a legitimate from-scratch equivalent.
- Import path: `ImportPcGameHelper.checkPermissionAndStartRead` is gated by
  `ManageStoragePermissionUtil`; if not granted it opens all-files access
  settings (Android 11+). Steam import posts a notification.
- File browser: `WinEmuFileSelectorActivity/Fragment` + `FileDirAdapter`, with
  media/USB mount receivers; `.exe` selection feeds the import helper.
- P1 mapping in GameHubX: SQLite `games` table, local `.exe` import, all-files
  access prompt (`FilesAccess` Java helper + JNI), live `statvfs` storage bar.
  No user accounts, server catalog, or GameHub endpoints.

## Batch 5 findings — P2 runtime and containers (2026-11-16)

- `BottleMetadata`: `name`, `winePath`, `tag`. `Container` reads/writes
  `metadata.json` and an optional `patch_version.txt` in the container dir.
- `ContainerManager`: containers live under `filesDir/usr/home/containers` with
  a `containers.json` registry; layout is `drive_c` plus a
  `dosdevices/c:` symlink to it; `.base-container` marks a base container and
  `.destroyed` marks deletion; repair can merge a base container.
- `WineComponentInstaller`: installs a component directory into the container
  (`drive_c/windows` or container root depending on layout) and removes those
  files on uninstall; `DependencyManager` tracks per-DLL dependencies and uses
  a `file_checksum` field.
- Downloads: MD5 via `FileUtils`, and the Room `Kernel` row carries `fileMd5`
  (Batch 3). Server-backed URLs; no publishable component mirrors.
- Adoption: our own JSON manifest with SHA-256 (stronger), per-game containers,
  safe tar.gz extraction, `drive_c`/`dosdevices/c:` layout, metadata.json.
  Wine bootstrap, base-container semantics and component merging are P3.

## Batch 6 findings — P3 launch/session/display (2026-11-16)

- `ProgramLauncher` runs `/system/bin/linker64` (optionally prefixed with
  `box64`) with the extracted imagefs as cwd. GameHub does **not** use PRoot; it
  relies on proprietary `libvfs.so`/`libsandboxfs.so` and imagefs extraction
  (`.tar.xz`/`.zst`, `usr/.installed`). Our open path uses PRoot + binds
  instead.
- `WineHelper` commands: `wine wineboot`, `wine winecfg -v <ver>`,
  `wine msiexec /i <msi>`, `wine regsvr32 /s <dll>`,
  `wine uninstaller --remove <uuid>`, `wineserver -k`, `wineserver -w`.
- `ProgramController`: starts `XServer` through `X11View` (Android surface),
  calls `start(rootPath, [])`, `startUI`, `stop`; sends touch/mouse/key events;
  launches `wine start /b /unix <program>` plus optional `jwm`/Steam; exits via
  callback.
- `EnvironmentController`: `PATH` with box64/wine bins, `LD_PRELOAD` VFS,
  `DISPLAY=:0`, `HOME`, `USER=steamuser`, `TMPDIR`, XDG vars,
  `WINEPREFIX`/`WINEPREFIX_BASE`, `WINEESYNC`, `WINEDEBUG=-all`,
  `WINELOADERNOEXEC`, `WINE_DISABLE_FULLSCREEN_HACK`,
  `WM_SERVER_PATH=<root>/tmp/.wm/V0`, `PERF_SERVER_PATH=/tmp/.perf/V0`,
  `ANDROID_SYSVSHM_SERVER=/tmp/.sysvshm/SM0`, `BOX64_*`, `VKD3D_*`, audio
  (Pulse/ALSA), `SteamGameId`.
- `X11Controller`: `X11View` + `TouchInputManager`, surface format, fullscreen,
  window realized/unrealized callbacks, SHM path, `XServer.stop`.
- Adoption: `SessionPlan` builds a PRoot command (`-0 -r <rootfs>`, binds for
  game/container/runtime, `wine64 <exe>`), open Wine/Box64/DXVK env, and
  bootstrap commands (`wineboot`, `wineserver -k/-w`). The X-server fork +
  Android surface presentation is the P3 G2 gate; `gamehubx-supervisor` owns
  process-group cleanup and PSS sampling.

## Batch 7 — gap audit against the reference (2026-11-16)

| Claim | Reference evidence | Verdict |
|---|---|---|
| No open Wine/Box64/PRoot/X-server pack in the repo | `lib/arm64-v8a` ships only proprietary `libvfs.so`, `libwinemu.so`, `libxserver.so`; no Wine/Box64/PRoot binaries and no `assets/winemu` payload. `ImageFsHelper` extracts `.tar.xz`/`.zst` into `filesDir/usr` and writes `usr/.installed`. `DependencyManager` downloads components (`download_archive`, `install_exe`, `install_msi`, `cab_extract`, `archive_extract`) with URL/`file_checksum`/size and a `.downloaded` marker. | **True:** the reference has an install mechanism, but the pack is proprietary/server-backed; there is no open pack in our repo. |
| No Android foreground session service/surface bridge | `WineActivity` is declared with `android:process=":wine"`, `Theme.WineUI`, `sensorLandscape`; it hosts `X11View` and calls native `XServer.start(rootPath, [])`, `startUI`, `surfaceChanged`, `sendTouch/Mouse/KeyEvent`, `stop`; `libxserver.so` is the bridge. There is no Wine foreground service. | **Partly inaccurate:** the session is a dedicated-process **Activity**, not a foreground service. GameHubX lacks both the Activity/process host and the surface bridge. |
| No Play Now wiring | GameDetail Play (`SendClickStartGameEvent` in `GameDetailHeadViewHolder`/`GameDetailActivity`) → `GHPcEmuLaunchStrategy`/`PcEmulatorStrategy` → `IWinEmuService.s(WineActivityData)` → `WinEmuServiceImpl` starts `WineActivity` (`WinAPI.v(context, WineActivity.class)`). | **Fixed in P3:** GameHubX Play Now builds a `SessionPlan`, spawns via the supervisor, and shows a Session screen; missing pack files are listed explicitly. |
| No device run | Static reference cannot prove a device run; the reference app is working. Our APK has not been built or booted. | **True for us.** |
| Session plan reports missing runtime instead of launching | Reference validates/installs via `ImageFsHelper` (`.installed`) and `DependencyManager`/`WinEmuDownloadManager` (`.downloaded`, per-component checksums) before the session. | **True:** our plan only detects missing files; install + launch wiring is the remaining gate. |

Design implication: mirroring the reference means a `WineActivity`-style session
host in a dedicated process with an X11 surface view, not necessarily a
foreground service. The open pack must still be assembled from open components.

## Batch 8 — runtime pack install design (2026-11-16)

- The reference never bundles Wine/Box64: `ImageFsHelper` extracts an imagefs
  (`.tar.xz`/`.zst`) and writes `usr/.installed`; `DependencyManager` downloads
  per-component archives (`download_archive`, `install_exe`, `install_msi`,
  `cab_extract`, `archive_extract`) with `file_checksum`/size and a
  `.downloaded` marker; server URLs only.
- GameHubX local equivalent: a pack folder with `manifest.json` plus
  `<component>.tar.gz` archives (`local://` URLs accepted), installed through
  the P2 verifier/extractor into `<app data>/runtime`, with per-component
  markers. The pack binaries themselves remain external/open-source; no
  GameHub binaries or URLs are used.

## Batch 9 — P4 input/audio/settings (2026-11-16)

- `.icp` profiles (4 shipped) are JSON: `id`, `name`, localized names,
  `cursorSpeed`, `gameId`, `elements`. Elements: `type`
  (`BUTTON`, `D_PAD`, `STICK`, `RANGE_BUTTON`), `shape` (`CIRCLE`, `SQUARE`,
  `RECT`), `bindings`, `scale`, normalized `x`/`y`, `toggleSwitch`, `text`,
  `iconId`, optional `range`.
- Binding order for `D_PAD`/`STICK` is **up, right, down, left**. Bindings use
  `GAMEPAD_*`, `KEY_*`, `MOUSE_*`, or `NONE`. `RANGE_BUTTON` carries six
  bindings and an optional range (e.g. `FROM_0_TO_9`).
- Touch pipeline: `X11View`/`X11Controller` → `TouchInputManager`/
  `TouchEventDispatcher` → `XServer.sendTouchEvent`/`sendMouseEvent`/
  `sendKeyEvent`; `pcvirtualbtn`/`mapping` edit and host profiles.
- Audio: `EnvironmentController` sets Pulse (`PULSE_SERVER=<root>/tmp/.sound/PS0`,
  `PULSE_LATENCY_MSEC=60`) or ALSA (`ALSA_CONFIG_PATH=<imagefs>/etc/alsa.conf`,
  `ALSA_PLUGIN_DIR=<imagefs>/lib/alsa-lib`,
  `ANDROID_ALSA_SERVER=<root>/tmp/.sound/AS0`, `ANDROID_ASERVER_USE_SHM=true`).
  PulseAudio is launched with `module-aaudio-sink`.
- Settings keys observed: `resolution`, `fpsLimit`, `dxvk_version`, audio
  driver, Box64 config, DXVK device memory/FPS cap.
- Adoption: own `.icp` parser/mapping engine + builtin profiles; Pulse/ALSA
  env in `SessionPlan`; SQLite `game_settings`; no GameHub binaries/art.

## Batch 10 — P5 power/memory (2026-11-16)

- `WINEMU_CPU_AFFINITY` is set to `~((1 << iB) - 1)` when a CPU-reserve config
  is positive (excludes the first `iB` CPUs).
- `WINEMU_MEMORY_LIMIT` is set when the per-game memory limit is > 0.
- Perf path: `PerfPlugin`/`PerfEventListener` over `/tmp/.perf/V0` reports FPS
  ticks and window start/stop present events to `ProgramController`.
- Additional perf/power env: `MANGOHUD`, `DXVK_HUD`, `DXVK_ASYNC`,
  `DXVK_STATE_CACHE_PATH`, `DXVK_CONFIG_FILE`, per-game DXVK device memory,
  RenderDoc capture.
- No PSS/onTrimMemory instrumentation found in the winemu core; PSS diagnostics
  come from our supervisor (sibling GameLite baseline: ~84 MB PSS).
- Adoption: OS-level `RLIMIT_AS` and `sched_setaffinity` instead of proprietary
  `WINEMU_*` env; static DXVK FPS cap; focus suspension; PSS Diag;
  `docs/POWER_MEMORY.md` holds the matrix.

## Batch 11 — Winlator-Ludashi runtime adoption (2026-11-16)

- Source: https://github.com/StevenMXZ/Winlator-Ludashi, default branch `5.0`,
  GPLv3; fork of Winlator Bionic (Pipetto-crypto), Winlator lineage.
- Provides the missing open session stack: Java `xserver`, `xenvironment`,
  `xconnector`, `renderer`, `inputcontrols`, `sysvshm`, `winhandler`, `services`;
  native `cpp/proot`, `cpp/winlator`, `cpp/virglrenderer`, `cpp/wfg`; PulseAudio
  (`libgn_pulse*.so`); top-level `input_controls/*.icp`.
- Runtime assets are downloaded at build: `imagefs.tar.zst` (two parts, from
  `Other-backup/winlator-imagefs-v2`) and Proton 11 ARM64EC (SHA-256 verified);
  components from `Winlator-Contents`.
- `.icp` profiles match the reference format; the reference's X server/audio are
  Winlator-family components.
- License is GPLv3. Decision: GameHubX becomes GPL-3.0-only and adopts the
  session layer; package spoofing (Ludashi/RedMagic), Steam/itch/GOG, LSFG and
  OpenXR are not adopted. See `docs/WINLATOR.md`.
- Vendored at commit `527848f555c4349b26578e714acd4654b6dbd956`:
  Java/Kotlin/res/scripts/MidiSynth/LICENSE/notices (~11 MB). Native `cpp/`,
  `jniLibs/`, assets and the session activity manifest remain W2.









