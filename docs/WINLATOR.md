# Winlator-Ludashi adoption plan

Decision (2026-11-16): GameHubX is licensed **GPL-3.0-only** and will reuse the
open Winlator session stack for the P3/P4 runtime and surface bridge.

Source: https://github.com/StevenMXZ/Winlator-Ludashi (default branch `5.0`,
GPL-3.0; fork of Winlator Bionic by Pipetto-crypto, Winlator lineage).

## Why

The repo contains the pieces GameHubX was missing and could not derive from the
proprietary GameHub reference:

- **X server + surface:** Java packages `xserver`, `xenvironment`, `xconnector`,
  `renderer`, plus `XServerDisplayActivity`.
- **Input:** `inputcontrols`, `ControlsEditorActivity`, `ExternalControllerBindings`,
  top-level `input_controls/*.icp`.
- **Audio:** `libgn_pulse*.so` (PulseAudio) and Android sysvshm.
- **Runtime:** native `cpp/proot`, `cpp/winlator`, `cpp/virglrenderer`, `cpp/wfg`;
  imagefs/Proton download tasks with SHA-256 verification.
- **Container model:** `container`, `contents`, `components`, `storage`,
  `winhandler`, `services`.

## Not adopted

- Package-name spoofing builds (Ludashi benchmark / RedMagic Genshin). These are
  performance-detection hacks; GameHubX keeps `dev.gamehubx`.
- Steam/itch/GOG store integrations, LSFG, OpenXR, cloud/streaming — optional,
  out of scope for v1.

## Target architecture

```
┌──────────────────────────────┐   Intent/JNI    ┌──────────────────────────┐
│ :app (Rust eframe launcher)  │ ──────────────► │ :session (Winlator GPLv3)│
│ library, import, settings,   │                 │ XServerDisplayActivity   │
│ touch editor, diagnostics    │ ◄────────────── │ xserver/renderer/input   │
└──────────────────────────────┘   exit/status   │ PulseAudio, PRoot/Wine   │
                                                └──────────────────────────┘
```

- `:app` stays the egui launcher (GameHubX UI, SQLite library, import, settings,
  diagnostics, touch preview).
- `:session` is a vendored Android library module from Winlator 5.0 containing
  the XServer activity, Java X server, input controls, renderer, PulseAudio
  libs and native PRoot/Wine wrappers.
- Play Now hands the game to the session activity; our SQLite row reaches the
  Winlator container/shortcut layer via a small adapter (create/import container,
  pass exe/cover IDs).
- The Rust `gamehubx-runtime` session plan and `gamehubx-supervisor` remain as
  fallback/diagnostics; Winlator owns the actual game process and surface.

## Integration phases

| Phase | Work | Exit |
|---|---|---|
| W1 | Sparse-vendor Winlator source, add `:session` Gradle module, license/notices, build in CI | APK builds with both modules |
| W2 | Launch `XServerDisplayActivity` from a debug intent with a test container; imagefs/Proton setup | Session activity opens without crash |
| W3 | Wire Play Now: adapt GameHubX game to a Winlator container/shortcut; exit cleanup | Imported `.exe` starts in session |
| W4 | Map per-game settings (resolution/FPS/DXVK/audio/touch profile) onto Winlator settings; route touch to its X server | Sound/touch working |
| W5 | Device G2 test (≥30 fps), P5 baseline, remove duplicated runtime plan where Winlator owns it | G2/G3/G4 evidence |

## Build and CI

- Add `android/session` (or a separate Gradle module) with Winlator's Java,
  resources, CMake native sources and `jniLibs` (PulseAudio).
- Keep the Rust cdylib `:app` NativeActivity; both modules ship in one APK.
- Gradle tasks download imagefs/Proton/assets with SHA-256 verification (mirror
  Winlator's build script; do not invent URLs).
- CI remains build-only: Gradle assemble + artifact; no local APK builds.
- Expect a materially larger APK and build time; cache Gradle/NDK and NDK
  components.

## Licensing and compliance

- Project: GPL-3.0-only (`LICENSE` added).
- Preserve Winlator's `LICENSE` and `THIRD_PARTY_NOTICES.txt` in the vendored
  tree; publish source for the shipped GPL binary.
- Third-party components keep their licenses: Wine (LGPL), Box64 (MIT), DXVK
  (zlib), PRoot (GPLv2, separate executable/source offer), PulseAudio (LGPL),
  Mesa/Turnip, FEXCore. Verify each notice before release.
- GameHub Lite decompiled material remains local-only; never commit it.
- Do not copy the Ludashi/RedMagic package-spoofing behavior.

## W1 progress (2026-11-16)

- Sparse-vendored `thirdparty/winlator` at commit
  `527848f555c4349b26578e714acd4654b6dbd956`: Java/Kotlin sources (314 Java,
  189 Kotlin), `res/` (647 files), `scripts/`, `MidiSynth.jar`, `LICENSE`,
  `THIRD_PARTY_NOTICES.txt`. ~11 MB.
- Added `android/session` as a library module (`com.winlator.cmod`) and made
  `:app` depend on it. Root Gradle now uses AGP 8.8.0, Kotlin 2.0.21 and the
  Compose plugin; app minSdk raised to 28.
- CI has not built this yet; the first Gradle run is the compile gate.

## W2 progress (2026-11-16)

Done:

- Native sources vendored: `app/src/main/cpp` (14 MB, including OpenXR-SDK,
  patchelf, ludashix11, wfg, Steam Rust client), `jniLibs` (PulseAudio),
  `android_sysvshm`, `input_controls`, and the upstream build scripts.
- `adrenotools` vendored at `8483dfdaa2abf97ee89ad0e5f337e7b508550c6b`
  (BSD-2-Clause).
- `:session` now builds the Winlator manifest, Java/Kotlin, resources,
  `jniLibs` and native CMake targets with NDK `29.0.14206865`.
- CI installs NDK 29 (plus 27 for cargo-ndk), prepares glslang/SPIR-V tools,
  sparse-fetches upstream `assets/`, downloads and verifies imagefs + Proton 11,
  and fetches the SHA-pinned component tools.
- App Settings now has **Winlator Session**, which starts Winlator's
  `MainActivity` through a method on `GameHubXActivity` (avoids native-thread
  `FindClass` failures). The Winlator launcher icon is also present during W2.

Pending / risks:

- No local Gradle/APK build; the first CI W2 run is the compile gate for the
  native tree, manifest merge, resources and Rust JNI.
- The manifest is merged as-is; it adds the Winlator launcher activity and
  store/Xr components that W3–W4 may trim.
- Target SDK is 35 (app) while Winlator targets 28; storage/foreground behavior
  needs the device test.
- Session boot still requires the fetched assets, imagefs extraction and a
  device run; neither has been verified here.
- Assets are third-party binaries fetched at build time; licenses/notices need
  a release pass (Steam binaries in particular).

## W3 progress (2026-11-16)

Done:

- Added `WinlatorBridge` in `:session` (`android/session/src/main/java/...`):
  checks the imagefs and container, writes a temporary `.desktop` shortcut
  with `Exec=env WINEPREFIX=… wine "<exe>"`, and launches
  `XServerDisplayActivity` with `container_id`, `shortcut_path` and
  `shortcut_name` — the same route as Winlator's file-manager "run file".
- `Play Now` on Android now calls this bridge before building the Rust session
  plan. A successful launch leaves the launcher for Winlator; a missing
  imagefs/container or launch error returns to the Session screen with the
  Winlator error and a **Winlator Setup** button.
- The desktop build keeps the Rust `SessionPlan`/supervisor path as a fallback.
- Shortcut input is constrained: newlines/quotes in the exe path are rejected,
  display names are sanitized, and stale bridge shortcuts are removed.

Limits:

- W3 reuses the first Winlator container (normally created by onboarding); it
  does not create per-game containers yet.
- Resolution/FPS/DXVK/audio/touch settings are not mapped onto Winlator
  shortcuts yet (W4).
- No on-device run; the first CI APK and a real container launch are pending.

## W4 progress (2026-11-17)

Done (launcher-side mapping; device verification still pending):

- The launcher serializes its per-game settings into a settings JSON
  (`gamehubx_runtime::session_settings_json`) and passes it through JNI:
  `launchWinlator(exe, name, settingsJson)` → `WinlatorBridge.setSettings(...)`.
- `WinlatorBridge` now applies those settings to the first Winlator container
  before launch and saves it: resolution → `screenSize`, FPS cap → `DXVK_FRAME_RATE`,
  DXVK/VKD3D → `dxwrapper`, audio backend → Winlator's `pulse-audio-gn`/`alsa-oboe`,
  MangoHUD/DirectX-HUD/WINEDEBUG/WM/core-limit + custom `env_vars` merged into the
  container env; command-line args are appended to the `.desktop` shortcut `Exec`.
- The settings are cached on the bridge so the post-asset-download launch path
  (`RuntimeAssetsActivity`) reuses them without re-crossing JNI.
- Trimmed ~10 MB of never-built native sources from the vendored tree
  (`OpenXR-SDK`, `xr`, `proot`, `virglrenderer`, `ludashi-steam-*`, `steam_optional.c`);
  see `thirdparty/winlator/VENDORED.md`. Java/Kotlin, resources, manifest, jniLibs
  and the CMake build targets are untouched.

Limits:

- Still reuses the first Winlator container (no per-game containers yet).
- Live touch/HUD injection into the X server remains the G2/display-bridge gate.
- No on-device run; the Android/JNI changes are not compiled by the host CI gate.

## Risks

- Vendoring a full Android app is large; build integration (Compose/AndroidX,
  CMake, submodule `adrenotools`) may take several iterations.
- Winlator expects its own Application/container/runtime setup; the adapter is
  the main design risk.
- Imagefs/Proton come from third-party backup repos; verify SHA and licenses.
- GPLv3 source publication is mandatory.
