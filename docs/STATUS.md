# GameHubX status

Updated 2026-11-16. CI is build-only; no local APK builds. This file records
state, measurements and gaps; the target design lives in `PLAN.md`.

## Reference

- GameHub Lite 5.1.8 decompiled under `reference/gamehub-lite-v518-decompiled/`
  (`apktool/`, `resources/`, `sources/`; zip retained). Gitignored, local-only.
- Original design snapshot: `reference/main.rs.orig`.
- Salvage source for scaffolding/runtime: `/home/Project/GameHub/gamelite`.
- GameHub patch toolchain (`/home/Project/gamelite`) is dev-only; ReVanced
  patches (`/home/Project/GameHub`) are out of scope.

## License and runtime decision (2026-11-16)

- GameHubX is now **GPL-3.0-only** (`LICENSE`, workspace `license`).
- Runtime/session path resolved: adopt **Winlator-Ludashi 5.0** (GPL-3.0) as the
  XServer/input/audio/PRoot/Wine session layer. See `docs/WINLATOR.md`.
- Winlator source vendoring and the `:session` Gradle module are **in progress
  (W2)**: Java/Kotlin/resources/MIDI jar, native `cpp/` (OpenXR, patchelf,
  adrenotools, ludashix11, wfg, Steam Rust client), `jniLibs/`, and the full
  session manifest are vendored at commit `527848f...`. CI fetches assets,
  imagefs/Proton and component tools, then builds with NDK 29. App Settings has
  a Winlator Session entry that opens Winlator's MainActivity. The first CI
  build and any device run are still pending.
- W3 is wired on Android: Play Now writes a temporary Winlator `.desktop`
  shortcut for the game's exe and starts `XServerDisplayActivity` with the
  first existing container. Missing setup returns an error to the Session
  screen with a Winlator Setup button. Per-game containers/settings are W4.
- Package-spoofing builds (Ludashi/RedMagic) are explicitly not adopted.
- GameHub Lite decompiled material stays local-only and is not redistributed.

## P0 — Foundation

Done in this scaffold:

- Workspace (`cargo` 1.100.0, edition 2021, arm64 release profile).
- `gamehubx-core`: `GameId`, `Game`, `GameKind`, stable FNV IDs, `local_<hex>`
  display IDs; unit tests.
- `gamehubx-platform`: Android/desktop files root, data dir, battery level,
  storage stats (statvfs), all-files access helpers.
- `gamehubx-app`: eframe 0.30 UI split into app/theme/widgets/overlays/screens;
  desktop bin + Android `cdylib` with `android_main` and
  `android-native-activity`.
- Android Gradle shell (`dev.gamehubx`, compileSdk 35, minSdk 26, arm64 only),
  NativeActivity host, manifest permissions, launcher icons.
- GitHub Actions build-only workflow producing `gamehubx-debug-apk`.
- Bug fixes from the design review:
  - B1 dashboard tile row is horizontally scrollable.
  - B2 uninstall removes by `GameId` and clamps selection/index.
  - B3 import modal validates empty names, resets `name`/`intro`/`pend`.
  - B4 per-game `exe`/`intro`, per-game derived local ID (no hard-coded ID).
  - B5 dashboard gear opens app settings instead of the empty PC screen.
  - B7 responsive fit: the fixed 1612×720 design is scaled to the viewport via
    egui `set_pixels_per_point` (clamped 0.25–4.0), so the design fits any
    landscape screen instead of being cropped.
  - Removed the per-frame `games.clone()`; tiles are rebuilt on change.
- Downloads screen count made consistent with its rows.

## P1 — Library (implemented, on-device verification pending)

Done:

- `gamehubx-storage`: SQLite (`rusqlite` bundled), WAL, 4 MB page cache,
  `mmap_size=0`, `user_version=1`; upsert/list/search/delete/update_exe.
  Schema mirrors the reference's local-library concepts (`t_game_library`:
  local game ID, last launched) but with our own columns.
- Persistence wired into the app: library loads on start; import upserts;
  uninstall deletes; startup-path changes update the row. Import survives
  restart (covered by a file-reopen test).
- Real search over label/intro with `LIKE` escaping; results open game detail.
- Data dir via Java `nativeSetFilesDir(getFilesDir())`; desktop fallback to
  `$XDG_DATA_HOME/gamehubx` or `~/.local/share/gamehubx`.
- All-files access: Java `FilesAccess` (`granted`/`request`) + JNI helpers; the
  file browser shows a grant/recheck prompt instead of silently showing an
  empty root.
- Live storage: `statvfs` used/total/available with byte formatting; battery
  stays live. App settings shows library DB state and file-access state.
- Storage tests: schema roundtrip, favorite ordering, search escaping,
  delete/update_exe, file-reopen persistence.

Gaps:

- No on-device check yet for JNI files dir, permission grant/return, import,
  persistence, search, and storage stats.
- Storage screen uses Used/Available/Total; the reference's Game/Media/Other
  categories need per-category sizing (P2+).
- Play Now and cover images are still placeholders. Downloads are implemented in
  P6 below; launcher gamepad focus is implemented in P7.

## P2 — Runtime install (implemented, Wine bootstrap pending)

Done:

- `gamehubx-runtime`: JSON component manifest (`schema 1`) with validation:
  required package version, unique safe names, HTTPS URLs, 64-hex SHA-256,
  positive size.
- Streaming SHA-256 verification (64 KiB chunks), declared-size check, disk
  preflight, per-component install markers, no silent version updates.
- Safe tar.gz extraction: absolute paths, `..` traversal, escaping
  symlinks/hardlinks, unknown entry types, and oversized entries are rejected.
- `ComponentSource` trait with `HttpSource` (HTTPS) and an in-memory source for
  host tests.
- `ContainerManager`: per-game containers keyed by `GameId` hex, `metadata.json`,
  `drive_c`, `dosdevices/c:`; create/open/repair/delete/list. Repair restores
  layout and missing/corrupt metadata.
- Integration test: install → verify marker → create container → delete.

Reference alignment (`docs/SCAVENGE.md` Batch 5):

- Reference uses `metadata.json` (`name`, `winePath`, `tag`), containers under
  app files, `drive_c` + `dosdevices/c:`, `.base-container`/patch markers, and
  merges component files into `drive_c/windows` via `WineComponentInstaller`.
  Downloads carry MD5 `file_checksum`.
- GameHubX uses SHA-256, per-game containers, and keeps pack components separate
  until the P3 prefix bootstrap.

Gaps:

- `HttpSource` compiles but is not exercised (no live download test); tar.gz is
  the only archive format.
- Bootstrap commands and component merging are planned in P3 but not executed
  against real Wine; patch-version/`.base-container` semantics are not ported.
- Runtime is not wired into the app data dir/UI; the supervisor backend exists
  but there is no Android session service yet (P3).
- No on-device verification.

## P3 — Launch/session (backend implemented; G2 blocked)

Done:

- Reference launch/display analysis recorded in `docs/SCAVENGE.md` Batch 6:
  env controller, `ProgramLauncher`/`linker64`, `WineHelper` bootstrap commands,
  `ProgramController`, `X11View`/`XServer`, touch/mouse/key bridge.
- `SessionPlan`: PRoot command (`-0 -r <rootfs>`, binds for game/container/
  runtime, `wine64 <exe>`), open Wine/Box64/DXVK env (`WINEPREFIX`,
  `WINEESYNC`, `DXVK_FRAME_RATE`, `DISPLAY=:0`, WM/perf/SHM socket paths),
  display plan, and missing-runtime detection.
- Bootstrap commands: `wineboot`, `wineserver -k`, `wineserver -w`; bootstrap
  step list (`create-prefix` … `write-config`).
- Component merge/remove mirroring `WineComponentInstaller`: `drive_c`
  components merge at container root, flat components into
  `drive_c/windows`; symlinks rejected; per-component file markers for exact
  removal.
- `gamehubx-supervisor`: spawn in a dedicated process group, wait/try_wait,
  group `SIGKILL` and reaping on terminate/Drop; PSS parsing from
  `smaps_rollup`/`smaps`.
- Play Now is wired (legacy Rust path): `launch_game` builds a `SessionPlan`, repairs/creates the
  container, spawns through the supervisor, and opens a Session screen. Missing
  runtime files are listed with exact paths; Stop/Back terminates the process
  group; PSS is sampled every 500 ms.
- Runtime pack installer: App Settings → Runtime Pack, or the Session screen’s
  “Open Runtime”. It reads `manifest.json` and `<component>.tar.gz` archives
  from `/storage/emulated/0/GameHubX/runtime-pack` (fallback:
  `<app data>/runtime-pack`), verifies SHA-256/size, and installs into
  `<app data>/runtime` on a background thread. Required components:
  `proot`, `wine` (with `wine64`/`wineserver`), `rootfs`, `xserver`.
  Local manifests use `local://` URLs (HTTPS is also accepted).
- CI backend check now includes `gamehubx-supervisor`.

Gate:

- **G2 is not met.** There is no open runtime pack in the repo (Wine, Box64,
  PRoot, X-server), no device run, and no Android surface presentation. The
  reference's `libvfs`/`linker64` path is proprietary and is not reused.
- Session host: the reference uses `WineActivity` in a `:wine` process (not a
  foreground service) with `X11View`/`XServer`/`libxserver.so`; GameHubX has
  neither the host nor the surface bridge.
- Play Now now attempts a launch: with a complete pack it spawns the session,
  otherwise it opens the Session screen listing the missing files. There is no
  pack download/install flow. On Android, W3 routes Play Now through Winlator's
  `XServerDisplayActivity` instead (see W2/W3 below).
- Runtime acquisition is now a local installer, but the open pack binaries are
  still external: no Wine/Box64/PRoot/X-server pack is bundled or downloaded.
- P4 adds DXVK application, PulseAudio launch, and touch preview/test, but live
  game injection still needs the surface bridge.

## P4 — Input/audio/settings (backend + settings implemented; live session pending)

Done:

- Reference `.icp`/audio/settings analysis recorded in `docs/SCAVENGE.md`
  Batch 9.
- `gamehubx-input`: `.icp` JSON parser/validation, builtin “Gamepad” and
  “Keyboard & Mouse” profiles, user profile loading from
  `<app data>/inputcontrols/*.icp`, and a hit-test mapping engine for
  BUTTON/D_PAD/STICK/RANGE_BUTTON (press/release, axes, toggle).
- Audio: `AudioBackend` Pulse/Alsa wired into `SessionPlan` env with the
  reference paths and Pulse latency. Pulse mode generates `default.pa`
  (`module-aaudio-sink`), starts PulseAudio through PRoot, and is stopped with
  the session.
- DXVK selection is applied at launch: merges `<runtime>/dxvk` into the
  container (`drive_c/windows` layout) and removes it when set back to
  “Default”; missing DXVK is reported before launch.
- Per-game settings persisted in SQLite `game_settings` (user_version 2):
  resolution, FPS cap, DXVK version, audio backend, touch profile.
  Game Settings rows cycle/save them; launch uses resolution/FPS/audio/DXVK.
- Touch Controls screen (`S::Touch`): profile preview, drag-to-move editing,
  “Save as custom” to `<app data>/inputcontrols/<id>.icp`, and a test mode that
  feeds real pointer events into the mapping engine and shows emitted inputs.
- Tests: input 10, storage 8 (settings roundtrip/delete, downloads, app setting), runtime 45 (DXVK,
  Pulse/ALSA, audio config, memory/affinity limits).

Gaps:

- Live touch injection into the game/X server is still missing; the engine is
  wired to the preview/test surface, not the P3 X bridge.
- PulseAudio binary and `module-aaudio-sink` must come from the open pack; no
  audio binary is bundled. The launch/config path is wired.
- Touch editor is drag-only (no element creation/deletion/scale/binding editor).
- P3 G2/device remains blocked; no playable sound/touch verification.

## P5 — Power/memory (instrumentation + controls implemented; device baseline pending)

Done:

- `docs/POWER_MEMORY.md`: budgets, baseline procedure and measurement matrix.
- Diagnostics screen (`S::Diag`, Settings → Advanced): app PSS breakdown,
  session/audio PIDs, session PSS, battery, storage, library/runtime state,
  focus and repaint policy.
- Focus suspension: no UI work or polling while unfocused; 60 s repaint.
- Repaint budgets: 15 s idle, 2 s session, 1 s diagnostics, 250 ms only while
  installing.
- Per-game memory limit applied as `RLIMIT_AS` in the supervisor `pre_exec`
  (inherited by PRoot/Wine); per-game big-core affinity applied with
  `sched_setaffinity` using a cpufreq-derived mask; FPS cap via
  `DXVK_FRAME_RATE`; Developer Options rows cycle/save both.
- PSS reader covers the app and the main session PID.

Gaps:

- G3/G4 are not verified: no reference/device baseline, no APK/device run.
- Session PSS aggregates only the main PID, not the Wine child tree.
- No thermal/FPS governor or `onTrimMemory`/memory-pressure callback yet.
- Big-core detection depends on cpufreq; session launch errors if unavailable.
- Session measurements still require the P3 open pack and surface bridge.

Local checks run 2026-11-16:

- `cargo metadata --no-deps` OK; workspace manifests parse.
- `cargo fmt --all -- --check` clean.
- `cargo check -p gamehubx-platform --target aarch64-linux-android` OK. This
  caught and fixed an Android-only JNI bug (`JValueOwned::Bool(1)` →
  `Bool(true)`) and removed a dead import.
- `cargo check -p gamehubx-app` (host) OK; three float-literal fallback
  warnings in the touch editor/widgets were fixed.
- `cargo test -p gamehubx-platform` → 6 passed.
- `aapt2 compile --dir android/app/src/main/res` succeeds for the app resources
  (including the new adaptive/monochrome icon).
- No Gradle/CMake/APK run locally; W2 native/manifest and the P7 signing/release
  path are CI-gated.
- `cargo test -p gamehubx-core -p gamehubx-platform -p gamehubx-storage` →
  17 passed (core 3, platform 6, storage 8).
- `cargo test -p gamehubx-app` → 1 passed (i18n translation/fallback/choice cycle).
- `cargo test -p gamehubx-download` → 4 passed (HTTPS validation, hash/commit,
  mismatch cleanup, status roundtrip).
- `cargo test -p gamehubx-runtime` → 45 passed, including DXVK apply/remove,
  Pulse/ALSA env, audio config/command, memory/affinity limits,
  install/verify/session tests.
- `cargo test -p gamehubx-input` → 10 passed (profile parse/validation,
  button/dpad/stick/range mapping, builtins).
- `cargo test -p gamehubx-supervisor` → 5 passed (exit code, group terminate,
  PSS parsing/read).
- Total backend tests: 81 (plus 1 app i18n test).
- `Cargo.lock` resolved eframe/egui 0.30.0, winit 0.30.13,
  android-activity 0.6.1, android_logger 0.14.1, chrono 0.4.45,
  rusqlite 0.32, ureq 2.12, tar 0.4, flate2 1.1, sha2 0.10, fs2 0.4.
- CI now runs a host `cargo check` over core/platform/storage/download/runtime/
  supervisor/input before the APK build, so backend code is compile-gated;
  the app also links runtime, download, supervisor and input.
- The app crate compiles on the host; the Android app target and `cfg`
  JNI/Java/Gradle paths were not compiled locally. CI is their first compile
  gate (host tests run on a non-Android target).

Not yet verified / gaps:

- The APK has not been built or booted on a device yet.
- P3 G2 (display bridge, ≥30 fps) is blocked on an open runtime pack and
  on-device testing. Play Now/session wiring exists, but there is no Android
  session host or surface presentation.
- Downloads are a real persisted queue; launcher gamepad focus, localization,
  adaptive icon and tag signing are implemented but not device/CI verified.
- P7 release items remain unverified: adaptive icon rendering, controller
  buttons beyond D-pad/center, Chinese font/layout, and the signed tag APK.
- The fit scaling needs an on-device visual check (font legibility, hint
  positions) on the target phone.
- No on-device PSS/power baseline for GameHub Lite or GameHubX yet; P5 G3/G4
  are not measured, and `docs/POWER_MEMORY.md` has placeholder rows.
- Session PSS covers only the main PID; Wine child-tree aggregation is missing.

## P6 — Downloads/settings (implemented; device/network pending)

Done:

- `gamehubx-download`: HTTPS-only resumable downloads using a `.part` file and
  Range requests, declared-size and optional SHA-256 verification, cancellation,
  and queued/downloading/paused/completed/failed states.
- Storage `user_version=5`: `downloads` table (title, URL, destination, bytes,
  status, hash, error, timestamps) plus insert/list/next-queued/progress/status/
  pause/requeue/delete/clear/reset methods and `busy_timeout`.
- App `DownloadQueue`: one background worker, persisted queue resumed on launch,
  interrupted tasks reset to queued, progress throttled to 250 ms, and
  interactive pause/resume/retry/remove/clear-completed actions.
- Downloads screen is real: Download Soon, Downloading/Paused, Failed, and
  Completed sections with bytes, percent, speed, completion time, retry, pause/
  resume, remove, and clear history. No demo rows remain.
- Runtime integration: **Download components** enqueues HTTPS components from
  the runtime manifest into `<app data>/downloads/<name>.tar.gz`; **Install
  runtime pack** installs from the local pack first, then downloaded archives.
  This is the end-to-end manifest → queue → verify → install path.
- Per-game settings and app settings were already implemented in P1/P4/P5.
- Tests: download 4, storage 8 (download-queue roundtrip and app-setting
  roundtrip), so backend total is 81.

Gaps:

- There is no game-store download source in GameHubX; the reference downloads
  game data from its own server. P6's queue is used for runtime components
  (and accepts any HTTPS spec internally).
- No live network test ran locally; HTTPS/Range/resume is compile- and
  unit-tested only.
- Downloads run in the app process; there is no Android foreground service or
  notification, so backgrounding may kill them. Interrupted tasks are requeued
  and resume on the next launch.
- Installing is a second user action after the downloads finish; clearing
  completed history leaves the downloaded archives so installation still works.

## P7 — Polish (implemented; release/device verification pending)

Done:

- Gamepad focus: focusable widgets register with `focus.rs`; Android gives the
  first content control focus when none exists, so D-pad spatial navigation and
  D-pad-center/Enter activation work through egui. `GameHubXActivity` remaps
  controller A/B/Y/Start to Enter/Escape/Y/M; D-pad passes through winit.
  Hints switch to controller labels on Android. See `docs/GAMEPAD.md`.
- Localization: `i18n.rs` supports English and Simplified Chinese, detects the
  system language (Android `Locale`/desktop `LANG`), persists the choice in the
  new `settings` table, and translates screen titles, buttons, rows, hints, and
  major direct labels. A GB2312/common-CJK subset of Noto Sans CJK is bundled
  (`assets/ui/fonts/GameHubX-CJK.ttf`, OFL) and loaded only for Chinese to limit
  memory.
- Adaptive icon: `mipmap-anydpi-v26`/`v33` adaptive icon with background,
  foreground and monochrome layers; existing PNGs remain the pre-26 fallback;
  manifest now sets `roundIcon`.
- Release signing: Gradle reads `GAMEHUBX_KEYSTORE_*` env and signs release
  builds when configured. CI builds debug on pushes/PRs and a signed release on
  `v*` tags, uploading `gamehubx-release-apk`. Docs: `docs/RELEASE.md`.
- App-setting storage migration (`user_version=5`) and an i18n unit test.

Gaps:

- Localization covers English and Simplified Chinese; some formatted/dynamic
  strings and RTL layouts remain English/unhandled.
- Controller A/B/Y/Start mapping is best-effort because NativeActivity may route
  input on the native queue; D-pad/center are the verified launcher path.
  Sticks/triggers are not mapped, and no controller was available for testing.
- The adaptive icon XML/vector compiles with `aapt2` but has not been rendered
  on a device/launcher or inspected for masking/safe-zone issues.
- No tag CI run or signed release APK exists yet; release secrets are not
  configured in this workspace.

## Next (release-candidate verification)

1. Run the W2/W3 CI build: NDK 29 native tree, manifest merge, fetched assets
   and the Gradle APK. Fix the first compile/merge errors.
2. On-device W2 check: open Settings → Winlator Session, finish onboarding,
   extract imagefs/Proton, create a test container, and confirm the session
   activity opens without crashing.
3. On-device W3 check: import an `.exe`, Play Now, and confirm Winlator starts
   the game with the created container; verify returning to GameHubX leaves no
   stale Rust session.
4. Then W4: map GameHubX resolution/FPS/DXVK/audio/touch settings onto the
   Winlator shortcut and route live touch/audio.
5. On-device P1 check: grant all-files access, import `.exe`, restart, search,
   uninstall, verify storage stats.
6. Capture the GameHub Lite 5.1.8 dashboard PSS/power baseline and fill the P5
   matrix (idle, library, session, game).
7. P3 G2: measure the Winlator display bridge at ≥30 fps.
8. P6 verification: run a real HTTPS component download on device (resume,
   pause, retry, offline failure) and confirm the downloaded archive installs
   through Runtime; decide whether downloads need a foreground service.
9. P7 verification: switch English/简体中文 and check glyphs/layout, test
   controller D-pad/center/A/B/Y/Start, inspect adaptive/round/themed icons,
   configure the signing secrets, tag a release, and verify it with apksigner.

## Verification

Local (small, host-only): `cargo metadata`, `cargo fmt`, core/platform/storage/
download/runtime/supervisor/input unit tests plus the app i18n test when
dependencies are available. No local APK builds.

Device checklist (once the first CI APK exists): launch, landscape, drawer,
all-files grant, import `.exe`, dashboard tile, detail, settings, search,
restart persistence, uninstall, storage stats, download queue pause/resume/
retry/offline, runtime install from downloaded archives, language switch and
CJK glyphs, controller D-pad/center/A/B/Y/Start, adaptive/round icon, back
stack, modal cancel, rotation, pause/resume, trim memory, signed release APK.
