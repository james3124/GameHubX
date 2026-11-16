# R7 — Session cleanup plan (read-only inventory)

Status: analysis only. **Nothing has been deleted, moved or rewritten.** This
is the only file produced by R7 preparation; it becomes actionable only after
R1–R6 exit gates pass on device.

Scope: remove the temporary Winlator Java/Kotlin session path after Rust owns
runtime, process, audio, input and the display bridge. Keep a thin Android
surface host, the C ABI libraries still used, and all licenses/notices.

## 1. Inventory snapshot (current tree)

| Area | Current state |
|---|---|
| Java/Kotlin | 503 files (314 Java, 189 Kotlin), 21 top-level classes, 647 `res/` files, `MidiSynth.jar` |
| `:session` manifest | `android/session/src/main/AndroidManifest.xml` is an empty stub; `android/session/build.gradle.kts` points `manifest.srcFile` at the vendored Winlator manifest, so **the vendored manifest is the effective one** |
| Session process | `XServerDisplayActivity` is `android:process=":session"` (3,723-line activity) |
| Java packages (files) | xserver 111, ui 95, core 52, steam 44, stores 25, widget 24, contentdialog 14, components 14, feature 11, xenvironment 10, gog 10, contents 10, inputcontrols 9, xconnector 8, winhandler 7, bigpicture 7, renderer 5, container 4, storage 4, sysvshm 4, fexcore 4, midi 3, box64 3, math 2, lsfg 1, services 1 |
| Native `cpp/` | targets in CMake: `winlator`, `fakeinput`, `vulkan_renderer`, `gpuusage`, subdirs `patchelf`, `adrenotools`, `wfg`, `lsfg-import`, `ludashix11`; vendored but not in this CMake: `xr`, `OpenXR-SDK`, `ludashi-steam-client`, `ludashi-steam-launcher`, `steam_optional.c`, `proot`, `virglrenderer` |
| Native libs | `jniLibs/`: 6 PulseAudio libs (`libgn_*`); no bundled Steam/OpenXR libs |
| Manifest entries | `WinlatorApplication`, `MainActivity` (exported, no launcher filter), `XServerDisplayActivity`, `OnboardingActivity`, `StorageActivity`, `ControlsEditorActivity`, `ExternalControllerBindingsActivity`, `GpuTestsActivity`, `WinlatorLabsActivity`, `ContainersSettingsActivity`, `NotificationService`, `GpuTestWorkerService` (`:labs_gpu_test`), `ShortcutBroadcastReceiver`, `WinlatorFilesProvider`, `FileProvider` |

## 2. Removable after R1–R6, with blockers to rewire first

| Group | Remove (Java/Kotlin) | Remove (native) | Blocking references / rewiring required first | Gate |
|---|---|---|---|---|
| X server activity/UI | `XServerDisplayActivity` bulk, `xserver/`, `xenvironment/`, `xconnector/`, `renderer/`, display `widget/`, `ui/` session HUD/sidebar; shrink to a thin `SessionActivity` | none (C++ X11 stays) | `NativeX11View`/`NativeX11InputProxy extends XServer` still need Java `XServer` for `ScreenInfo`, `Pointer`, `Keyboard`, `Drawable`; `XServerView`/`VulkanXServerView`; `XServerDisplayActivity` owns `XEnvironment`, `SysVSharedMemoryComponent`, `XServerComponent`, `NativeX11Component`, `AdrenotoolsManager`; R6 surface/input ABI must replace these | R6 |
| Containers/contents | `container/` (`Container`, `ContainerManager`, `Shortcut`), `contents/` (`ContentsManager`, `Downloader`, profiles, driver managers), `components/`, container UI, `ImageFsInstaller` | none | Session module `WinlatorBridge` imports `Container`, `ContainerManager`, `ImageFs`; app `RuntimeAssetsActivity`/`GameHubXActivity` call it; `MainActivity`/`OnboardingActivity` create containers; `SettingsFragment` uses `ContentsManager`/`ImageFsInstaller`; `XServerDisplayActivity` + `GuestProgramLauncherComponent` read container/contents; R2 must own create/repair/merge/DXVK/driver extraction | R2 + R3 |
| input controls | `inputcontrols/` (`InputControlsManager`, `ControlElement`, `ControlsProfile`, `ExternalController`), `widget/InputControlsView`, `TouchpadView`, control editor UI | none (keep `fakeinput` until input ABI is proven) | `XServerDisplayActivity`, `ControlsEditorActivity`, `InputControlsFragment`, `SettingsFragment`, `ShortcutSettingsDialog`, `winhandler/WinHandler`, `xserver/Keyboard`; R5 `InputRouter` must feed the C ABI and own profiles | R5 |
| winhandler | `winhandler/` (`WinHandler`, task manager dialog/sidebar, process info) | none | `xserver/XServer`, `xserver/InputDeviceManager`, `widget/InputControlsView`, `TouchpadView`, `container/Container`, shortcut/container editors; task-manager feature must be dropped or reimplemented in Rust | R5 |
| Stores/Steam/GOG/big picture | `steam/` (44), `gog/` (10), `stores/` (25), `bigpicture/` (7), `feature/stores/steam/...` (11), store/big-picture activities/fragments/screens | `cpp/ludashi-steam-client`, `cpp/ludashi-steam-launcher`, `steam_optional.c`, Steam assets/notices | `MainActivity` menu/`SteamLibrary`/`SteamGameDetailFragment`; `ShortcutsFragment` SteamGridDB artwork + `SteamLibrary`; `XServerDisplayActivity` `SteamLaunch`/`SteamCloud`/`SteamSocial`/`SteamAchievements`/`SteamSession` hooks; `ui/library/*`; `storage/*`; `feature/.../LudashiSteamClient` loads an unbuilt `libludashisteam`; remove plain `.exe` Steam branches first | R7 (after branch removal) |
| XR | `XrActivity.java` (extends `XServerDisplayActivity`), XR layout/array/string ids | `cpp/xr/`, `cpp/OpenXR-SDK/` (already not in CMake) | `XServerDisplayActivity` calls `XrActivity.isEnabled`; `core/AppUtils` imports `XrActivity`; `res/layout/xserver_display_activity.xml`, `container_detail_fragment.xml`, `values/arrays.xml`; remove manifest XR `<uses-feature>` lines | R7 |
| LSFG/WFG | `lsfg/LosslessScaling.java`, `ui/settings/FrameGenerationSettings`, frame-generation UI in renderer dialogs | `cpp/lsfg-import`, `cpp/wfg`, `winlator/renderer/vulkan/lsfg/*`, `VulkanRendererWfg.cpp`, LSFG calls in `VulkanRendererContext.cpp`/`.h`, LSFG link in `ludashix11` | `XServerDisplayActivity`, `NativeX11View`, `VulkanXServerView`, `FrameGenerationSettings`; `ludashix11/CMakeLists.txt` links `wfg` and `native_renderer.cpp` has `frameGenEngine`; keep/drop decision required before native surgery; CMake text-replacement markers are brittle | R6 + decision |
| Services | `services/NotificationService`, `ui/settings/GpuTestWorkerService`, GPU-lab activities | `cpp/gpuusage` (HUD only, optional) | `MainActivity` starts `NotificationService`; `XServerDisplayActivity` uses its exit `PendingIntent`; manifest service/foreground-service entries; replace exit notification in the thin host or drop it | R7 |
| Follow-on (not in scope but coupled) | `box64/`, `fexcore/`, `midi/` presets, HUD `widget/gpu/KgslBusy`, `core/ProcessHelper` executors | `fakeinput` only if Rust input replaces it | `GuestProgramLauncherComponent` (R3), `XServerDisplayActivity` HUD, `WinHandler` LD_PRELOAD; decide before deleting shared `core/` helpers | R3/R5 |

Native targets that **remain**: `cpp/winlator` (JNI display/drawable/pixmap/xconnector/sysvshm/process glue), `cpp/ludashix11` (C++ X11 core + renderer), `cpp/vulkan_renderer` core (minus LSFG/WFG), `cpp/patchelf` (driver patching), `cpp/adrenotools` (driver loading), `cpp/proot`, `cpp/virglrenderer`, `fakeinput` until R5 proves the input ABI, and PulseAudio `jniLibs` until R4 is wired. `proot`/`virglrenderer` are vendored but not referenced by the session CMakeLists; confirm their build/asset path before any cleanup.

## 3. What must remain

- **`:app` surface**: `GameHubXActivity` (NativeActivity, `libgamehubx_app`), `RuntimeAssetsActivity` only until R2 setup moves fully to Rust, `FilesAccess`, app manifest and launcher icon.
- **`:session` thin host**: exactly one exported Activity in `android:process=":session"` that receives the session Intent/Binder, creates/attaches/detaches the `Surface`, handles lifecycle, rotation, IME and permissions, and returns exit status. No fragments, Compose screens, static session registries or business state.
- **Bridge**: a minimal `SessionBridge`/Intent contract replacing `WinlatorBridge`; if R2/R3 still need Java, keep only `Container.id`/`ImageFs` path handoff, not managers.
- **C ABI wrappers still in use**: `libwinlator` (if any remaining JNI calls), `libludashix11`, `libvulkan_renderer` (core), `libadrenotools`, `libproot.so`/`proot-loader`, `libvirglrenderer`, `libfakeinput` (until R5), PulseAudio `libgn_*`. Each wrapper must be opaque, generation-checked and destroy-idempotent.
- **Manifest/res**: application entry (remove `WinlatorApplication` or replace with a minimal class), session activity, required permissions, themes/strings/layouts used by the thin host. Trim the rest only with a compile-verified `R`/resource sweep.
- **Licenses/notices**: `LICENSE`, `thirdparty/winlator/LICENSE`, `THIRD_PARTY_NOTICES.txt`, plus Wine (LGPL), Box64 (MIT), DXVK (zlib), PRoot (GPLv2), PulseAudio (LGPL), adrenotools (BSD-2), Mesa/Turnip, FEXCore notices. Keep an archive notice for removed components; GPLv3 source publication remains mandatory.

## 4. Safe removal order and gates

**Nothing is deleted in this phase.** Execute only after R1–R6 exit evidence exists.

1. **Freeze and baseline.** Tag the R0–R6 build, record app/`:session` PSS/RSS, FDs, threads, JNI refs, surface counts, and 50-cycle deltas. Gate: CI green; no uncommitted source changes in the session path.
2. **Dead/disabled code first (no launch-path effect).** XR sources/manifest/resources; dead Steam native dirs/`steam_optional.c`/`feature` loader; `gpuusage`/HUD if dropped. Gate: build; plain `.exe` 50 cycles; `/proc/<pid>/maps` shows no OpenXR/steam/wfg libs; APK shrink recorded.
3. **LSFG/WFG decoupling.** Remove Java controls, then native `wfg`/`lsfg-import`/LSFG sources and `ludashix11` linkage; rebuild Vulkan renderer without LSFG. Gate: 50 surface cycles, no `vkr_lsfg` symbols, frame pacing unchanged, CMake stops using network FetchContent.
4. **UI/setup surfaces after R2.** `MainActivity`, fragments, onboarding, settings, containers/contents/input editors, storage, big picture; move setup to the app/Rust; remove `WinlatorApplication` and unused manifest components. Gate: setup + launch from GameHubX; no exported Winlator activity; 50 cycles.
5. **Input/winhandler after R5.** Replace `InputControlsManager`/profiles and `WinHandler` task manager with Rust `InputRouter`; delete static listeners and Java input views. Gate: touch/gamepad/IME tests; no retained listener/queue growth; 50 cycles.
6. **Containers/contents after R2+R3.** Delete Java managers/installers/profiles; replace `WinlatorBridge` with the Rust session request. Gate: Rust install/repair/launch/delete; restart persistence; 50 cycles.
7. **Display/audio Java after R4+R6.** Reduce `XServerDisplayActivity` to the thin host; delete `xserver`, `xenvironment`, `xconnector`, `renderer`, display `widget`, audio components; keep only the surface/input JNI shim. Gate: rotation/resize/background/exit; 50 surface create/destroy cycles.
8. **Services/manifest/resources/dependencies last.** Replace `NotificationService` exit path or drop the notification; remove GPU-lab services, providers, receiver; prune 647 `res/` files, Compose/retrofit/glide/jsoup/zxing/conscrypt/bouncycastle/openjsse/junixsocket/MidiSynth if unused. Gate: release build, manifest merge, license audit, then the 200-cycle gate below.

Each step must land as a separate reviewable change with its own 50-cycle run; never combine a native target removal with a Java UI removal.

## 5. 200 launch/exit leak gate

Preconditions: release-like arm64 build on a fixed device/thermal state; plain `.exe`; R0 baseline captured; automated 4 × 50-cycle blocks (or 200 continuous); 20+ failure-path cycles interleaved; logcat/JNI/heapprofd/perfetto enabled.

Per cycle: cold app → launch → 60 s gameplay → exit → settle 5 s → sample. Record CSV row with app/session PSS, RSS, FDs, threads, PIDs, JNI counts, surface count.

| Metric | Measurement | Pass criteria |
|---|---|---|
| `:app` PSS/RSS | `smaps_rollup` (`Pss`, `Private_Dirty`, `Rss`), `dumpsys meminfo` | No monotonic growth; ≤16 MB per 50-cycle block; block-over-block slope ≈ 0; no single step >2 MB without explanation |
| `:session` PSS/RSS | `/proc/<pid>/smaps_rollup`, `dumpsys meminfo` | Session PID gone within 5 s after exit, or cached PSS returns to baseline ±2 MB with no anon growth; Wine child tree aggregated |
| FDs | `/proc/<pid>/fd` count, app and session | After exit app returns to baseline ±2; per-cycle delta ≤2; no upward trend over 200; session FD count gone with PID |
| Threads | `/proc/<pid>/status` `Threads` | After exit app baseline ±1; no named Rust/Java reader/audio/input thread survives; no upward trend |
| Surviving PIDs | `ps -A`, process group/cgroup, `pidof` | Zero `:session`, `proot`, `wine*`, `wineserver`, `pulseaudio`, X server; zero zombies/defunct; process group empty |
| JNI refs | ART JNI heap (`dumpsys meminfo`/heapprofd), debug `-Xcheck:jni`, native allocation trace | No per-cycle global-ref growth; no retained `Activity`/`Context`/`Surface`/`jobject`; local refs bounded; no JNI warnings/table overflow |
| Native surfaces | `dumpsys SurfaceFlinger --list`, gfxinfo, C ABI counters | Exactly one create/destroy per cycle; zero session layers after exit; zero live EGL/Vulkan contexts, pixmaps, X windows, shm mappings; gralloc/dmabuf stable |
| Failure paths | Missing runtime, bad exe, SIGKILL session, kill Wine/wineserver, rotation/resize mid-launch and mid-game, background/foreground, `am send-trim-memory`, audio start failure, input disconnect, exit during startup | Same cleanup assertions as a normal cycle; no crash/ANR; no leak beyond the normal-cycle budget; retry succeeds |
| Stability | Crash/ANR logs, FPS/cold start, thermal/battery | 200/200 cycles, zero crash/ANR; cold start ≤2.5 s; no >10% FPS regression; thermal not throttled |

Final pass: all rows hold for 200 cycles and each failure path; heapprofd shows flat native/Java allocations; no new global JNI references; evidence attached to the R7 review. Any single monotonic metric fails the gate.

## 6. Open decisions and risks

1. **Dedicated `:session` process** — keep it (isolation, killability) despite IPC and a second process's memory; Android may cache the process, so assert on PSS/PID rather than assuming death.
2. **C++ reuse vs Rust rewrite** — reuse the ~130k-line X11/render/PRoot stack behind the C ABI; rewriting it is high risk and not needed for leak-free ownership. The ABI must stay opaque, idempotent and generation-checked.
3. **Thin host shape** — strip `XServerDisplayActivity` in place or introduce a new `SessionActivity`; either must preserve Surface lifecycle, rotation, PiP, IME, back/exit and failure reporting.
4. **Java input coupling** — `NativeX11View` still needs a Java `XServer` input proxy; R5 must supply pointer/keyboard/screen state through the ABI before `xserver/` can go.
5. **Feature loss** — Steam/GOG/itch/big picture, XR, LSFG/WFG frame generation, task manager, HUD, GPU lab, driver/content UI, multiple renderer/audio backends. LSFG is user-visible; decide drop vs opt-in (keeping it contradicts this removal list).
6. **R2 completeness** — Rust does not yet own download/extraction/container creation/component merge; dual ownership must end before deleting Java managers.
7. **Build fragility** — `VulkanRendererContext.cpp` is modified by CMake string substitution; `lsfg-import` uses network `FetchContent`; removing either needs a simpler CMake, not more patches.
8. **Resource/dependency pruning** — 647 resources, Compose, retrofit/glide/jsoup/zxing/conscrypt/bouncycastle/openjsse/junixsocket, `MidiSynth.jar`; delete with `R`-class and manifest verification, not by directory guess.
9. **Licensing** — GPLv3 source publication and notices for every shipped native component; an archive notice for removed components avoids losing attribution.
10. **Measurement** — no on-device baseline yet; session PSS excludes the Wine child tree; JNI global-ref introspection is limited outside a debug build. The 200-cycle run is time/thermal expensive and must be scheduled, not inferred.
11. **Vendor updates** — sparse-vendored Winlator patches must be reapplied after upstream merges; keep removed code documented so it can be restored if a feature returns.

## 7. Non-actions

No Java/Kotlin, native, manifest, resource or Gradle file is modified by this plan. The only artifact is this document; deletion begins at step 2 after the R1–R6 gates and the step-1 baseline are signed off.
