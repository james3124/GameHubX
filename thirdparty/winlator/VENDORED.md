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
