# Power and memory budgets

Updated 2026-11-16. Instrumentation and controls are implemented; **no on-device
baseline or G3/G4 verification yet** (no APK build here, P3 display bridge
blocked).

## Implemented controls

| Area | Control |
|---|---|
| Idle UI | Repaint every 15 s; clock/battery/storage refresh; no animations |
| Unfocused/paused | No UI work; repaint every 60 s; polls paused |
| Session screen | Repaint every 2 s; PSS sampled on each repaint |
| Diagnostics | Repaint every 1 s while the Diag screen is open |
| Runtime install | 250 ms poll only while installing; background thread |
| Downloads screen | 500 ms refresh/repaint only while queued/downloading; worker does network I/O |
| Android session | `XServerDisplayActivity` runs in `:session`; Diagnostics shows app and session RSS/threads/FDs from `/proc` |
| FPS cap | `DXVK_FRAME_RATE=<fps>` from per-game settings |
| Memory cap | Per-game `memory_limit_mb` applied as `RLIMIT_AS` in the supervisor `pre_exec` (inherited by PRoot/Wine children) |
| CPU affinity | Optional big-core mask computed from `/sys/.../cpufreq/cpuinfo_max_freq` (top ≥80% of fastest core); applied with `sched_setaffinity` in `pre_exec` |
| Audio | PulseAudio/ALSA env; Pulse shim launched with the session and stopped with it |
| Launcher memory | SQLite WAL with 4 MB cache and no mmap; cached dashboard tiles; bounded touch event list |

The Diag screen (Settings → Advanced) shows app PSS, session PSS/PID, audio PID,
battery, storage, library/runtime state, focus and repaint policy.

## Targets

Relative to GameHub Lite 5.1.8 on the same device (baseline not yet captured):

| Metric | Target | Measurement |
|---|---|---|
| Dashboard PSS | < 50% of reference; interim ≤ 120 MB | `dumpsys meminfo` |
| Idle drain | < 50% of reference over 1 h | `dumpsys batterystats` |
| Session service/audio overhead | ≤ 20 MB PSS | `dumpsys meminfo <pid>` |
| Cold start | ≤ 2.5 s | `am start -W` |
| Idle redraw | 0 frames when idle/unfocused except scheduled ticks | eframe repaint stats |
| APK size | ≤ 30 MB debug arm64 | CI artifact |

## Baseline procedure (device)

1. Install GameHub Lite 5.1.8 and GameHubX (CI artifact).
2. Let each app idle on the dashboard for 60 s.
3. Capture `dumpsys meminfo <pkg>` (total PSS, anon/file/shmem, top mappings).
4. Capture `dumpsys batterystats --reset`, idle 1 h, then
   `dumpsys batterystats <pkg>`.
5. Start a session; sample PSS every 30 s for 10 min; record FPS if available.
6. Record raw numbers and device/build in `docs/STATUS.md`.

## Matrix (to fill on device)

| State | Reference PSS | GameHubX PSS | Reference drain | GameHubX drain |
|---|---|---|---|---|
| Dashboard idle 60 s | — | — | — | — |
| Library/search scroll | — | — | — | — |
| Import browser | — | — | — | — |
| Session idle (no game) | — | — | — | — |
| Game running 10 min | — | — | — | — |

## Notes and gaps

- PSS sampling currently covers the main session PID; Wine child tree aggregation
  (wineserver, game process) is not implemented.
- No thermal/FPS governor yet; FPS cap is static per game.
- No Android `onTrimMemory`/memory-pressure callback besides focus suspension.
- Big-core detection depends on cpufreq exposure; some devices return none.
- Session measurements require the P3 open pack and surface bridge.
