# GameHubX — Detailed Plan

Draft 2026-11-16.

Inputs:

- `main.rs` — the untested egui design recreation of GameHub Lite's launcher.
- `reference/gamehub-lite-v518-decompiled/` — decompiled GameHub Lite 5.1.8
  (`apktool/`, `resources/`, `sources/`; zip retained in `reference/`). Read-only,
  local-only, never committed.
- `/home/Project/GameHub/gamelite` — proven Rust Android launcher scaffold
  (Slint shell, NativeActivity + Gradle, CI, SQLite library, runtime/installer/
  supervisor crates, measured 84 MB PSS). Primary salvage source.
- `/home/Project/gamelite` — native APK/ARSC/AXML/DEX toolchain. Dev-only reference
  for inspecting the decompiled app; not part of the launcher.
- `/home/Project/GameHub` — ReVanced patch work. Out of scope.

## 0. Executive summary

GameHubX is a from-scratch Rust Android launcher that recreates GameHub Lite's
landscape dashboard and PC-game flow (import `.exe` → container → play), while
measurably beating the original on memory and power.

Two tracks run in parallel:

1. **Shell** — egui/eframe 0.30 UI, library, import browser, settings, downloads,
   storage management, controller/touch input.
2. **Runtime** — PRoot + Wine + Box64 containers assembled from open-source
   components, X-server display bridge, touch mapping, audio.

The APK is built only in GitHub Actions. Nothing in this repo is built locally;
local work is limited to small host tests.

## 1. Goals, constraints, success criteria

### Goals

- G1 — Visual/UX parity with GameHub Lite's launcher shell (dashboard, search,
  PC games, downloads, game management, file import, game detail, per-game
  settings, app settings, drawer/modals).
- G2 — Import and launch Windows `.exe` games on arm64 Android via an isolated
  per-game Wine container.
- G3 — Low memory: launcher PSS below 50% of GameHub Lite 5.1.8 on the same
  device; no per-frame allocations in steady state.
- G4 — Low power: event-driven repaint, no polling loops, no background work
  beyond an active session; idle drain measured and budgeted.
- G5 — Sideloadable, arm64-only APK produced by CI; deterministic versioning.
- G6 — Clean-room implementation: no GameHub code, binaries, art, fonts, or CDN
  endpoints shipped.

### Constraints

- Android 8.0+ (reference is minSdk 26, targetSdk 34); v1 targets arm64 only.
- Landscape-only shell (`sensorLandscape`), matching the reference.
- Sideload distribution: `MANAGE_EXTERNAL_STORAGE` is acceptable; Play Store
  policy is not a target.
- No root; containers run under PRoot.
- No local APK builds; CI is the only compile/package gate.
- `reference/` is ~1 GB of decompiled commercial material and must stay out of git.

### Success criteria (initial, to be baselined on the user's device)

| Metric | Target | How measured |
|---|---|---|
| Dashboard PSS | < 50% of GameHub Lite 5.1.8; interim budget ≤ 120 MB | `dumpsys meminfo` + PSS breakdown |
| Idle drain | ≤ 50% of reference over 1 h dashboard idle | `dumpsys batterystats` |
| Cold start to dashboard | ≤ 2.5 s | `am start -W` / logcat timestamps |
| Session service overhead | ≤ 20 MB PSS | `dumpsys meminfo :pc` |
| APK size (debug, arm64) | ≤ 30 MB | CI artifact |
| UI frame pacing | 60 fps on interaction, 0 redraws when idle | eframe repaint stats |

The 50% numbers are relative to a baseline captured on the target phone before
optimization; they are not absolutes until that baseline exists.

## 2. Current state (2026-11-16)

- `reference/` created; zip moved in and extracted:
  - `apktool/` 704 MB (smali, raw resources, native libs)
  - `sources/` 260 MB (jadx Java)
  - `resources/` 67 MB (decoded res, assets, manifest)
- `main.rs` — 405-line egui prototype; no Cargo project yet. Code targets
  eframe/egui **0.30** APIs (`Rounding`, 3-arg `rect_stroke`/`galley`,
  `TextEdit::margin(Vec2)`, `Frame::inner_margin(f32)`), verified against
  docs.rs 0.30. It has not been compiled or run.
- Sibling GameLite has P0–P2 done: build-only CI APK, PSS baseline 84 MB,
  SQLite library, icon cache, runtime manifest/installer, import browser.
- Reference facts established: package `gamehub.lite`, v5.1.8 (65),
  minSdk 26, landscape, permissions list, NativeActivity-free Java activities;
  fonts `d_din_pro_600_semibold.otf` (commercial — do not ship) and
  `roboto_medium_numbers.ttf` (Apache-2.0); `assets/inputcontrols/profiles/*.icp`
  (4 mapping profiles); key layouts `activity_pc_game_setting.xml`,
  `activity_win_emu_setting.xml`, `activity_settings.xml`,
  `game_activity_game_library_main.xml`, `llauncher_activity_main_guide_{1..4}.xml`.

## 3. Architecture

```
┌───────────────────────────────┐        ┌──────────────────────────────┐
│ Launcher process              │        │ Session process (:pc)        │
│ gamehubx-app (eframe/egui)    │        │ gamehubx-supervisor          │
│  UI ── core ── storage        │        │  spawn PRoot/Wine/Box64      │
│  platform (JNI) ── runtime    │        │  X-server child, audio shim  │
│  diagnostics (PSS/power)      │        │  FPS cap, affinity, cleanup  │
└──────────────┬────────────────┘        └──────────────┬───────────────┘
               │ JNI (files dir, battery, statvfs,       │
               │ all-files access, lifecycle)           │
┌──────────────┴────────────────────────────────────────┴───────────────┐
│ Android framework: NativeActivity host, Gradle APK, foreground service │
└───────────────────────────────────────────────────────────────────────┘
```

Rules:

- The launcher never runs the game runtime in-process. The session service is a
  separate `:pc` process so the shell can be frozen while a game runs and game
  memory cannot leak into the UI process.
- UI state is owned by `gamehubx-app`; durable state lives in
  `gamehubx-storage`; Android-specific access is isolated behind
  `gamehubx-platform` traits with desktop stubs.
- Runtime components are downloaded and verified (SHA-256) into the app's files
  dir; no GameHub binaries, no GameHub CDN.

### Key flows

**Import `.exe`:** Files screen → pick `.exe` → validate → insert `Game` +
`GameSettings` + container row → dashboard tile → detail → settings.

**Launch:** Detail "Play Now" → supervisor plans prefix/env → start session
service → PRoot spawns X server + Wine → surface presented full-screen →
launcher pauses/repaints stop → on exit, cleanup + PSS/power sample recorded.

## 4. Project structure

```
GameHubX/
├── Cargo.toml                     # workspace
├── Cargo.lock                     # committed (app, not library)
├── rust-toolchain.toml            # 1.100.0 + aarch64-linux-android
├── .gitignore                     # /target, /reference, Gradle artifacts
├── main.rs                        # design snapshot; moved in P0 (see below)
├── crates/
│   ├── gamehubx-app/
│   │   ├── Cargo.toml             # cdylib + rlib + bin; eframe 0.30
│   │   └── src/
│   │       ├── lib.rs             # run(); #[cfg(android)] android_main()
│   │       ├── main.rs            # desktop entry -> gamehubx::run()
│   │       ├── app.rs             # App state, eframe::App, navigation stack
│   │       ├── theme.rs           # colors, spacing, type scale
│   │       ├── widgets.rs         # btn/chip/row/toggle/hints primitives
│   │       ├── overlays.rs        # drawer, import modal, tile popup
│   │       └── screens/
│   │           ├── dashboard.rs
│   │           ├── search.rs
│   │           ├── pc_library.rs
│   │           ├── downloads.rs
│   │           ├── management.rs
│   │           ├── files.rs
│   │           ├── detail.rs
│   │           ├── game_settings.rs
│   │           └── app_settings.rs
│   ├── gamehubx-core/             # ids, domain types, validation, filtering
│   ├── gamehubx-input/            # .icp profiles, builtin profiles, mapping engine
│   ├── gamehubx-download/         # resumable HTTPS engine, task/status model
│   ├── gamehubx-storage/          # SQLite library, settings, migrations, download tasks
│   ├── gamehubx-platform/         # JNI bridge, battery/statvfs, paths, scan
│   ├── gamehubx-runtime/          # components, installer, prefix, env, plan
│   └── gamehubx-supervisor/       # session service, spawn, PSS/power, cleanup
├── android/
│   ├── settings.gradle.kts
│   ├── build.gradle.kts
│   ├── gradle.properties
│   └── app/
│       ├── build.gradle.kts
│       └── src/main/
│           ├── AndroidManifest.xml
│           ├── java/dev/gamehubx/GameHubXActivity.java
│           ├── res/mipmap-*/ic_launcher.png
│           └── jniLibs/            # CI output, gitignored
├── assets/
│   └── ui/fonts/                  # redistributable fonts only
├── docs/
│   ├── PLAN.md                    # this file
│   ├── STATUS.md                  # phase state, measurements, open gaps
│   ├── POWER_MEMORY.md            # budgets, baseline procedure, matrix
│   └── SCAVENGE.md                # what was taken/ignored and licensing
├── reference/                     # gitignored; decompiled app + zip
└── .github/workflows/android.yml  # build-only APK CI
```

Dependency direction (no cycles):

```
app -> core, input, platform, storage, runtime, supervisor
supervisor -> runtime
runtime -> core
storage -> core
platform -> core
```

## 5. Crate responsibilities

| Crate | Owns | First deliverables |
|---|---|---|
| `gamehubx-core` | `GameId`, `Game`, `GameKind`, `GameSettings`, `LibraryQuery`, derived `local_<hex>` IDs, path validation | domain types + unit tests; fixes hard-coded IDs |
| `gamehubx-input` | `.icp` profiles, builtin/user profile loading, touch hit-test mapping | parser/validation + mapping engine tests |
| `gamehubx-storage` | SQLite (`rusqlite` bundled, WAL, 4 MB page cache, no mmap), migrations, icon cache (12 MB LRU) | library + per-game settings persistence |
| `gamehubx-platform` | JNI: files dir, all-files access, battery, statvfs, scan helpers, lifecycle; desktop stubs | permission flow + live stats |
| `gamehubx-runtime` | component manifest, SHA-256 verify, installer, prefix plan, env plan, container metadata | installer parity with GameLite |
| `gamehubx-supervisor` | foreground service, process spawn/exit, PSS/power sampling, FPS cap, cleanup | spawn a dummy child; PSS readout |
| `gamehubx-app` | eframe shell, screens, navigation, overlays, theme, input | design parity + bug fixes |

Reuse policy: port GameLite code where it is generic (storage pragmas, SHA-256
installer, env planning, PSS diagnostics, icon cache). Rename to `gamehubx-*`,
keep identifiers stable once published. Do not copy Slint UI code if staying on
egui.

## 6. UI plan

### Screen map from `main.rs`

| `main.rs` state | Planned module | Notes |
|---|---|---|
| `S::Dash` | `screens/dashboard.rs` | tiles, selection description, import CTA |
| `S::Search` | `screens/search.rs` | add real filtering over library |
| `S::Pc` | `screens/pc_library.rs` | empty state now; becomes PC library |
| `S::Dl` | `screens/downloads.rs` | real download queue + history |
| `S::Mgmt` | `screens/management.rs` | live storage breakdown (statvfs) |
| `S::Files` | `screens/files.rs` | import/startup-path picker |
| `S::Detail` | `screens/detail.rs` | per-game ID, Play Now wiring |
| `S::Set` | `screens/game_settings.rs` | per-game settings, uninstall |
| `S::App` | `screens/app_settings.rs` | app settings |
| `S::Session` | `screens/session.rs` | launch status, missing runtime files, PSS, stop |
| `S::Runtime` | `screens/runtime.rs` | local runtime pack install/status |
| `S::Touch` | `screens/touch.rs` | profile preview, drag edit, save custom, engine test |
| `S::Diag` | `screens/diag.rs` | PSS/PID/battery/storage/repaint diagnostics |
| layers | `overlays.rs` | drawer, import modal, popup |
| helpers | `widgets.rs`, `theme.rs` | primitives and tokens |

### Design tokens

Keep the prototype palette (`BG #070E20`, `CARD #181E2E`, `DIM #8C94A5`,
`ACC #3B82F6`, chip `#D8DBE2`, frame `#0A1226`) as the starting point, then
sample the reference at runtime for parity. Base layout is 1612×720; introduce
a scale factor for other landscape resolutions instead of hard-coded pixels.

### Input

Touch is primary. Map the visual hints (`A`, `B`, `LB`, `RB`, `Y`, `≡`) to
Android key/controller events in P6; until then, keep them as affordances and
make every action reachable by touch. Back button and `Escape`/`B` both pop
overlays/screens.

### Bug backlog (from `main.rs` review)

| ID | Issue | Location | Fix |
|---|---|---|---|
| B1 | Dashboard tile row overflows with >4 imported games (fixed 250 px tiles, no scroll) | `main.rs:153-170` | wrap row in `ScrollArea::horizontal()` |
| B2 | Uninstall leaves stale `cur`/`sel`; later screens can read the wrong game | `main.rs:295-298`, `370-373` | remove by `GameId`; clamp indices after mutation |
| B3 | Import modal never resets `name`/`intro`/`pend`; empty names are accepted; `intro` is discarded | `main.rs:347-364` | validate/trim, derive name from file stem, reset on confirm/cancel |
| B4 | Library is `Vec<String>`; `exe`/`intro` are global; every game shows the same hard-coded local ID | `main.rs:20`, `252-255`, `260` | per-game record; derived `local_<16hex>` ID |
| B5 | Gear next to "Import PC games" opens the empty PC screen | `main.rs:185` | route to app settings (verify intent against reference) |
| B6 | Downloads/history and storage labels are hard-coded; battery is live only | `main.rs:214-229` | real download store + `statvfs` |
| B7 | Fixed 1612×720 window and pixel sizes | `main.rs:400-402` | **Implemented:** fit-to-screen `set_pixels_per_point` scaling (0.25–4.0) |
| B8 | Search query is unused; no results list | `main.rs:188-198` | filter `LibraryQuery` incrementally |
| B9 | Gamepad hints are labels only | `main.rs:390-395` | map controller keys; keep touch fallback |
| B10 | File browser has no permission handling, filtering, or empty states | `main.rs:232-256` | all-files access flow, `.exe` filter, error states |
| B11 | No Android lifecycle handling (pause/suspend, memory trim) | `app.rs` integration | stop repaints on pause, persist on trim |
| B12 | `/sys` battery and `/storage` root are desktop assumptions | `main.rs:23-31` | platform trait with Android implementations |
| B13 | "Play Now", Steam, "Add to Home screen" are no-ops | `main.rs:167`, `265-267`, `369` | **Play Now wired** to SessionPlan/supervisor/Session screen; Steam and "Add to Home screen" remain placeholders |

API baseline: pin `eframe = "0.30"` and `chrono = "0.4"`. Do not upgrade to
0.31+ casually: `Rounding` → `CornerRadius`, `rect_stroke`/`galley` gain a
`StrokeKind`-style argument, and `Frame::none()` deprecations will break this
code.

## 7. Data and storage

SQLite file: `<filesDir>/gamehubx/library.db` (WAL, `synchronous=NORMAL`,
4 MB page cache, `mmap_size=0`).

```
games(
  id INTEGER PRIMARY KEY,        -- stable hash or autoincrement mapped to GameId
  kind TEXT NOT NULL,            -- pc_exe | steam | android
  label TEXT NOT NULL,
  intro TEXT NOT NULL DEFAULT '',
  exe_path TEXT,                 -- nullable for non-PC entries
  cover_path TEXT,
  container_id TEXT,
  favorite INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  last_played INTEGER
)
containers(id TEXT PRIMARY KEY, wine_version TEXT, prefix_path TEXT, created_at INTEGER)
downloads(id INTEGER PRIMARY KEY, title TEXT, url TEXT, dest TEXT,
          bytes_done INTEGER, bytes_total INTEGER, status TEXT, sha256 TEXT,
          error TEXT, created_at INTEGER, completed_at INTEGER)
game_settings(game_id INTEGER PRIMARY KEY, resolution TEXT, dxvk_version TEXT,
              fps_cap INTEGER, env_json TEXT)
settings(key TEXT PRIMARY KEY, value TEXT)
```

Icon cache lives on disk beside the DB with a 12 MB LRU budget. Migrations use
`PRAGMA user_version`. Import is idempotent per `(kind, exe_path)`.

Runtime layout:

```
filesDir/gamehubx/
├── library.db
├── icons/
├── containers/<game-id>/prefix/
├── runtime/{wine,box64,dxvk,turnip,xserver}/
└── downloads/
```

## 8. Android integration and build

### UI stack decision

Keep egui/eframe 0.30 for design parity. eframe 0.30 added Android support:
`NativeOptions` gains `android_app: Option<winit::platform::android::activity::AndroidApp>`
and the app exports `android_main`. Required feature: `android-native-activity`.
Risk: egui's Android PSS is unmeasured; P0 includes a memory spike. Fallback:
port the shell to Slint (the sibling's proven 84 MB path) if the eframe gate
fails.

### Android entry point

```rust
// crates/gamehubx-app/src/lib.rs
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: winit::platform::android::activity::AndroidApp) {
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    eframe::run_native(
        "GameHubX",
        options,
        Box::new(|_cc| Ok(Box::new(crate::App::new()))),
    )
    .unwrap();
}
```

Desktop entry (`src/main.rs`) calls the same `run()` used by `android_main`.
The Android artifact is a `cdylib` so Gradle can package it.

### NativeActivity host

Reuse the sibling pattern: `GameHubXActivity extends NativeActivity`, manifest
`meta-data android.app.lib_name="gamehubx_app"`, `sensorLandscape`,
`configChanges` for orientation/size/density, `exported=true`, launcher intent.
Add JNI methods for files dir, all-files access, battery, statvfs; log
`onTrimMemory`/`onLowMemory`.

Manifest permissions (sideload): `MANAGE_EXTERNAL_STORAGE`, storage read/write
for legacy API, `INTERNET`, `FOREGROUND_SERVICE`,
`FOREGROUND_SERVICE_SPECIAL_USE`, `POST_NOTIFICATIONS`; optional gamepad
feature.

### Gradle + CI

`android/` mirrors the sibling: AGP 8.7.2, Gradle 8.10.2, compileSdk 35,
minSdk 26, arm64-v8a only, Java 17, `useLegacyPackaging = true`.

`.github/workflows/android.yml`:

1. checkout, Rust stable + cache, JDK 17.
2. install Android platform 35, build-tools 35, NDK 27.
3. `cargo install cargo-ndk`.
4. `cargo ndk -t arm64-v8a -o android/app/src/main/jniLibs build --release -p gamehubx-app --lib`.
5. `gradle assembleDebug` (or `assembleRelease` with secrets on tags).
6. upload `gamehubx-debug-apk` artifact.

CI is build-only; no emulator/test jobs. Local verification is limited to small
host tests.

`.gitignore` must include `/target/`, `/reference/`, `android/.gradle/`,
`android/**/build/`, `android/app/src/main/jniLibs/`, `local.properties`.

## 9. Runtime plan

Adopt the GameLite findings (see its `docs/SCAVENGE.md`) and upstream open
components only.

| Piece | Choice | Gate |
|---|---|---|
| Container | per-game Wine prefix under PRoot | import → prefix created |
| Wine | arm64 Wine + Box64 (open builds) | `wineboot` completes |
| Components | manifest + SHA-256 + resumable download + installer | install/repair/delete |
| Display | Winlator-family X server fork (MIT/X11), not GameHub's `libxserver.so` | G2: 30 fps Wine app |
| GPU | Adreno/Turnip vs Mali/vendor probe → DXVK vs WineD3D | correct driver chosen |
| Input | `.icp` mapping vocabulary (D_PAD/BUTTON/STICK/RANGE_BUTTON; GAMEPAD_*/KEY_*/MOUSE_*), normalized coords | touch mapping profile |
| Audio | ALSA → AAudio shim | audio in session |
| Env | `WINEESYNC`, `DXVK_FRAME_RATE`, our own memory-limit/affinity names, per-title extras | measurable FPS/power delta |
| Session | foreground service in `:pc`, notification, exit cleanup | G3 |

Explicitly excluded: GameHub `libwinemu.so`, `libxserver.so`, CDN endpoints,
cloud streaming, pay/OTA/analytics, ReVanced patching.

## 10. Low-memory and low-power design

Techniques:

- Repaint on input only; clock every 30 s, battery/storage every 60 s; zero
  redraws when idle or paused.
- Stop all UI work in `suspended`/`onPause`; save on trim.
- No polling threads in the shell. Scanning, downloads, hashing run on workers
  with bounded channels and cancel-on-screen-exit.
- Icon cache capped at 12 MB; decode at display size; drop cover textures not
  visible for N frames.
- SQLite WAL, small page cache, no mmap; queries only on data change.
- Bounded per-frame allocations: reuse `Vec`s, avoid cloning the library each
  frame (the prototype does `self.games.clone()` per frame — fix in P0).
- During sessions: launcher frozen; FPS cap; `WINEESYNC`; optional CPU affinity;
  scoped wake lock; kill orphans on exit.
- Measure before optimizing: PSS breakdown (total/anon/file/shmem, top mappings)
  and battery stats; keep a Diag screen.

Baseline procedure (on the user's phone): install GameHub Lite 5.1.8 and
GameHubX, capture `dumpsys meminfo <pkg>` at dashboard after 60 s idle and
`dumpsys batterystats` over 1 h idle; record into `docs/STATUS.md`.

## 11. Phases and gates

| Phase | Deliverables | Exit criteria |
|---|---|---|
| P0 Foundation | workspace, toolchain, app crate, main.rs moved into `screens/`, theme/widgets, bug fixes B1–B5, Android shell, CI workflow, PSS/power baseline capture, Android CI APK boots | APK shows dashboard on device; CI green; baseline recorded |
| P1 Library | SQLite store, per-game model, import browser with permissions, live battery/storage, search, uninstall, persistence | **Implemented:** SQLite games table, persistence, search, permission prompt, live stats; on-device verification and memory gate pending |
| P2 Runtime install | component manifest/verify, installer, container create/repair/delete | **Implemented:** manifest validation, SHA-256/size verify, safe tar.gz install, per-game container layout/metadata/repair/delete; Wine bootstrap and UI wiring in P3 |
| P3 Launch | PRoot + Wine session, X display bridge, session host (dedicated-process Activity or foreground service), exit cleanup | **Backend + Play Now/session/Runtime install implemented:** plans, bootstrap commands, component merge, supervisor/PSS, local pack installer; **G2 not met** (open pack binaries and surface bridge absent, no device run) |
| P4 Input/audio | touch→mouse, `.icp` virtual controls, audio shim, FPS/resolution/DXVK settings | **Implemented:** `.icp` engine, preview/editor/test mode, DXVK apply, PulseAudio config/launch, per-game settings/UI; **not met:** X-game injection, bundled AAudio/PulseAudio binary, device verification |
| P5 Power/memory | budgets, suspend/freeze, affinity/FPS caps, measurement matrix | **Implemented:** Diag PSS, focus suspension, repaint budgets, RLIMIT_AS memory cap, big-core affinity, FPS cap, POWER_MEMORY matrix; **not met:** device baseline/G3/G4, thermal/child-PSS |
| W1 Session vendor | vendor Winlator 5.0 source, `:session` Gradle module, notices, CI build | **Done:** Java/Kotlin/res/MIDI, native tree, jniLibs and manifest vendored; `:session` builds in CI with fetched assets; first build still to run |
| W2 Session boot | open Winlator setup/container launcher; imagefs/Proton setup; then `XServerDisplayActivity` with a test container | setup activity opens, no crash; full test-container boot still pending |
| W3 Play wiring | adapt GameHubX game to Winlator container/shortcut; exit cleanup | **Implemented:** Play Now writes a temp `.desktop` and starts `XServerDisplayActivity` with the existing container; desktop keeps the Rust fallback; on-device launch pending |
| W4 Input/audio | map settings/touch to Winlator; route touch/audio | sound and touch work |
| W5 G2/baseline | device ≥30 fps, P5 matrix, remove duplicate runtime pieces | G2/G3/G4 evidence |
| P6 Downloads/settings | real queue/history/retry, per-game settings UI, app settings | **Implemented:** persisted download queue (resume/pause/retry/remove/clear), background worker, speed/progress/history UI, runtime manifest HTTPS → downloads → installer; settings screens already present; on-device/network verification pending |
| P7 Polish | gamepad mapping, localization, adaptive icon, release signing, docs | **Implemented (partial):** D-pad/center focus navigation + best-effort A/B/Y/Start remap, English/Simplified Chinese UI + persisted language, adaptive icon v26/v33, tag-based signing and release docs; CI/device release build pending |

## 12. Verification

- CI: Android build + artifact on every push; no local APK builds.
- Host: small unit tests for core (IDs, validation, filtering), storage
  migrations, installer hashing/extraction; keep test batches small.
- Device: manual checklist per phase (import, launch, settings, uninstall,
  rotation, pause/resume, trim memory, low storage, offline download).
- Diagnostics: PSS/power panel mirroring GameLite's Diag; record in
  `docs/STATUS.md`.

## 13. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| eframe Android memory/perf unknown | G3 miss | P0 spike; fallback to Slint shell; measure before deep UI work |
| Display bridge (X server) complexity | blocks launching `.exe` | highest-risk gate; reuse Winlator-family open code; scope one resolution/input path |
| Wine/Box64 per-title compatibility | low playable library | component versions + per-title env extras; publish compatibility matrix |
| Android 11+ storage restrictions | import/permissions | all-files access flow as reference; SAF fallback |
| Commercial font/art in reference | legal | no DIN Pro, no GameHub art; open fonts only |
| CI NDK/Gradle drift | build breaks | pin versions in workflow; cache; upload logs |
| No local build verification | late compile errors | small PRs, CI on every push, host tests |

## 14. Open decisions

| ID | Decision | Recommendation |
|---|---|---|
| D1 | egui/eframe vs Slint on Android | egui (design already written); confirm with P0 PSS gate |
| D2 | Runtime bring-up | **Resolved:** adopt Winlator-Ludashi 5.0 session layer (GPLv3); see `docs/WINLATOR.md` |
| D3 | Library storage | SQLite (matches reference and GameLite) |
| D4 | App/package identity | `dev.gamehubx`, binary `gamehubx`, lib `gamehubx_app` |
| D5 | v1 scope | PC `.exe` flow only; Steam/Android tiles are placeholders |
| D6 | Release signing | **Resolved:** CI signs on `v*` tags using `GAMEHUBX_KEYSTORE_*` secrets; unsigned release for local compile checks; see `docs/RELEASE.md` |
| D7 | Baseline device | the user's arm64 phone; record in `docs/STATUS.md` |
| D8 | Session host | **Resolved:** Winlator `XServerDisplayActivity` (upstream runs it in the main app process; only XR uses `:vr_process`), with our Rust launcher in the same APK |
| D9 | Project license | **GPL-3.0-only**; Winlator notices preserved, source published |

## 15. Licensing and legal

- `reference/` is decompiled commercial software: local reference only. Never
  commit the zip, extracted sources, resources, native libs, or art.
- Do not ship GameHub binaries (`libwinemu.so`, `libxserver.so`, etc.) or CDN
  URLs. Rebuild runtime from open sources with their licenses (Wine LGPL,
  Box64 MIT, DXVK zlib, Turnip/Mesa MIT, X-server forks MIT/X11).
- Fonts: `roboto_medium_numbers.ttf` is Apache-2.0 (keep notice); DIN Pro is
  commercial — replace with an OFL font (e.g., Inter).
- Do not copy GameHub strings/art wholesale; use the reference for terminology
  and layout behavior, implement independently.

## 16. Immediate next actions

1. Approve D1 and D4.
2. Scaffold the workspace and move `main.rs` into `crates/gamehubx-app` without
   behavior changes.
3. Fix B1–B5 and the per-frame `games.clone()` allocation.
4. Add the Android Gradle shell + CI workflow and get the first CI APK booting
   to the dashboard.
5. Capture the GameHub Lite 5.1.8 memory/power baseline on-device.
6. Then start P1 (SQLite library + real import flow).
