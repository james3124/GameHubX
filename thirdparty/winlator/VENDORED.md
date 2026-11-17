# Vendored Winlator-Ludashi source

Upstream: https://github.com/StevenMXZ/Winlator-Ludashi
Branch: `5.0`
Commit: `527848f555c4349b26578e714acd4654b6dbd956`
License: GPL-3.0 (see `LICENSE`, `THIRD_PARTY_NOTICES.txt`)

Included in this checkout:

- `app/src/main/java/` (Java/Kotlin sources)
- `app/src/main/res/`
- `app/src/main/scripts/`
- `app/src/main/AndroidManifest.xml` (merged into `:session` in W2)
- `app/build.gradle`, `app/gradle.properties`, `app/proguard-rules.pro`
- `app/libs/MidiSynth/MidiSynth.jar` (cn.sherlock/jp.kshoji MIDI classes)
- `LICENSE`, `THIRD_PARTY_NOTICES.txt`, root Gradle files
- `app/src/main/cpp/` (native XServer/renderer/PRoot/Wine wrappers)
- `app/src/main/cpp/adrenotools/` (vendored commit `8483dfda...`, BSD-2-Clause,
  including nested `lib/linkernsbypass` at `b10d485...`, BSD-2-Clause)
- `app/src/main/jniLibs/` (PulseAudio libs)
- `input_controls/`, `android_sysvshm/`
- `.github/scripts/` (native shader tool and build helper scripts)

Fetched at CI build time (not stored in the repo):

- `app/src/main/assets/` from upstream at commit `527848f...` (sparse
  checkout) and `imagefs.tar.zst` plus the Proton 11 archive from the upstream
  download URLs. The Proton archive is SHA-256 verified.
- Component tools from `component-tools.json` into `jniLibs/` (SHA-256
  verified per file).

`android/session` builds the Winlator Java/Kotlin/resources, native CMake
targets (NDK 29) and the manifest. W2 is build-gated by CI; on-device session
launch is not yet verified.

## Trimmed native sources (GameHubX)

The following `app/src/main/cpp/` directories were **not referenced** by the
build CMakeLists (`android/session/build.gradle.kts` compiles
`app/src/main/cpp/CMakeLists.txt`, which only adds `patchelf`, `adrenotools`,
`gpuusage`, `winlator`, `fakeinput`, `vulkan_renderer`, `wfg`, `lsfg-import`,
`ludashix11`), were not `#include`d by any built target, and were not loaded by
any `System.loadLibrary` reachable from the GameHubX launcher. They were removed
to shrink the vendored tree from ~28 MB to ~18 MB:

- `OpenXR-SDK/` — OpenXR SDK; XR is disabled and not adopted (see WINLATOR.md).
- `xr/` — native XR backend for the dormant `XrActivity` (not in the merged manifest).
- `ludashi-steam-client/`, `ludashi-steam-launcher/`, `steam_optional.c` — Steam launcher;
  store integrations are out of scope and CI does not run `prepare-steam-launcher.sh`.
- `proot/` — PRoot source; the runtime uses the imagefs-provided PRoot, not this build.
- `virglrenderer/` — VirGL renderer; unused by the shipped renderer path.

All Java/Kotlin sources, resources, the manifest, jniLibs, input control profiles,
CMake build definitions, and LICENSE/NOTICES were kept intact, so compilation is
unaffected. To restore any of these, re-checkout the paths at the pinned upstream
commit `527848f555c4349b26578e714acd4654b6dbd956`. The `R7_SESSION_CLEANUP.md`
inventory still describes the trimmed components for attribution purposes.
