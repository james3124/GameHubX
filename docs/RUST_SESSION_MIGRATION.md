# Winlator session → Rust migration plan

Status: proposal, not started. Goal: make a Rust `gamehubx-session-core`
own the PC session lifecycle so teardown is deterministic and leaks are
measurable, while reusing the large C/C++ X11/runtime pieces behind a narrow ABI
instead of rewriting them blindly.

## Implementation status (local, not pushed)

- S1 partially implemented locally:
  - one launcher icon
  - `XServerDisplayActivity` in `:session`
  - XR/OpenXR disabled/removed from the lean build
  - Steam native targets/menu and Steam assets removed from the lean path
  - user-triggered runtime asset download in Downloads / first game launch
- R0 started: Diagnostics reports launcher PID/RSS/threads/FDs and Winlator
  `:session` PID/RSS/threads/FDs from `gamehubx/session.pid`; stale PIDs are
  discarded.
- R1 scaffold: `crates/gamehubx-session` provides non-cloneable
  `SessionGuard`, `SessionRequest`, state transitions, cancellation and tests.
- R2 started:
  - Rust validates runtime asset readiness and required files
    (`.ready`, imagefs, Proton, container pattern), verifies the Proton SHA-256,
    reads marker commit, and writes the readiness marker.
  - Rust discovers/parses Winlator containers from
    `imagefs/home/xuser-*/.container` and can safely remove a container.
  - Downloads and Diagnostics use the Rust readiness/container view.
  - Java still owns the actual asset downloader, imagefs extraction, container
    creation and component/content managers; R2 is not complete.
- R3 started:
  - `WineLaunch::from_container` builds the Wine/Proton command, environment,
    working directory, memory limit and CPU affinity from a Winlator container.
  - `WineProcess` wraps the supervisor process group, exposes pid/wait/terminate
    and kills the group on drop.
  - Tests cover command construction, missing Wine, memory/affinity, and owned
    child reaping.
  - Java `GuestProgramLauncherComponent` still launches the Android session;
    R3 is not wired into the session host yet.
- R4 started:
  - `AudioPlan` writes `default.pa`, builds the PulseAudio command/environment
    and exposes the Pulse socket.
  - `AudioProcess` owns the supervisor process group and kills it on drop.
  - `SessionLaunch` combines Wine + optional audio; `SessionProcess` terminates
    Wine first and audio afterward, including on spawn failure/drop.
  - ALSA has no helper process. Java audio managers are still in the Android
    session path; R4 is not wired into the host yet.
- R5 started:
  - `InputRouter` is a single-owner, bounded 64-event ring with touches/keys/
    buttons/axes tracking. Focus-loss/stop emit releases, clear state, survive a
    full queue, and stop is idempotent; Drop clears all state.
- R6 started:
  - `DisplayBridge` owns at most one opaque surface handle/generation,
    enforces create/attach/resize/detach/destroy ordering, rejects stale
    callbacks, and destroys exactly once on Drop.
  - A mock backend validates 50 cycles; real C ABI/JNI wiring is not done.
- R7 inventory complete: `docs/R7_SESSION_CLEANUP.md` lists removable and
  retained Java/native pieces, blockers, removal order and the 200-cycle leak
  gate. No source has been deleted.
- Remaining work is integration, not new backend scaffolds: wire the Rust
  SessionHost into the Android `:session`, move downloader/container creation
  fully to Rust, and connect R3–R6 to the C ABI. R7 deletion begins only after
  R1–R6 device gates pass.
- Per user instruction, local commits are not pushed until the plan is
  implemented; the remote remains at the last pre-plan commit.

## 1. Goals and non-goals

Goals:

- One Rust owner for container/imagefs state, process spawning, environment,
  input routing, audio handles, and session teardown.
- A single Android session process per game, killable by the supervisor.
- No retained Java `Activity`/`Context`, JNI local references, surface objects,
  native renderer objects, threads, FDs, or PulseAudio handles after exit.
- Measurable pass/fail: PSS/RSS, FD count, thread count, JNI reference table,
  heapprofd, and repeated launch/exit cycles.
- Keep GPLv3 compliance and the existing Winlator notices.

Non-goals in the first migration:

- Rewriting the X11 server, Vulkan renderer, PRoot, virglrenderer, or Wine in
  Rust. Those are roughly 130k native lines and are not required to fix
  ownership leaks.
- Steam/GOG/XR/big-picture features.
- A new display protocol before the existing C ABI is stable.

## 2. Current boundary and leak surface

The current APK has two stacks:

- Rust `:app`: egui launcher, SQLite library, download queue, runtime layout,
  session plan, supervisor, input profile engine.
- Winlator `:session`: Java/Kotlin `XServerDisplayActivity`, X server
  (`xserver`, ~9k lines), containers/contents, input controls, winhandler,
  renderer, and C++ PRoot/Wine/display libraries
  (`winlator` ~25k, `ludashix11` ~31k, `proot` ~18k, renderer/virgl ~58k).

Known ownership risks to eliminate:

- Java static listeners and debug callbacks (`GlobalKeyEventDispatch`,
  `ProcessHelper`, window-modification listeners, content managers).
- Activity-owned renderer/surface/XServer graphs with asynchronous exit paths.
- JNI local-reference growth at the Rust/Java boundary and cached
  `Activity`/`Context` references.
- Java executors, handlers, PulseAudio threads, XServer epoll threads, and
  `winhandler` threads whose lifetimes are not joined to session exit.
- Native EGL/Vulkan contexts, pixmaps, X11 windows, and shared-memory mappings
  not destroyed on every failure path.

## 3. Slim the current Winlator path before migrating

Do this in small, reversible steps before porting ownership to Rust.

| Item | Decision | Why |
|---|---|---|
| Duplicate launcher icons | Remove Winlator `MainActivity`/`XrActivity` launcher filters; keep one GameHubX launcher | Only one installable app surface |
| Runtime assets | No automatic download at app start; download from Downloads or first game launch | APK stays ~43 MB instead of ~527 MB; no surprise 500 MB use |
| Session process | Put `XServerDisplayActivity` in `android:process=":session"` | Launcher memory and session memory do not accumulate in one process |
| XR/OpenXR | Remove native OpenXR build/xr sources; make `XrActivity` unsupported/no-op | Not needed for PC `.exe`; avoids loader/library memory |
| Steam/GOG/stores/big-picture | Strip menus, services, download workers and native Steam client when no store session is used | Avoid background services and large optional graphs in the game path |
| LSFG/WFG frame generation | Keep disabled/absent for the default `.exe` profile; make it opt-in later | Extra renderer work and GPU/CPU memory during gameplay |
| Drivers/components | Install only the selected graphics driver/DXVK/wrapper; drop unused `.tzst` payloads | Less extraction, disk and mmap pressure |
| Debug/log/GPU-lab services | Never start in a normal session | Avoids threads, log buffers and services in the session process |
| Runtime/container state | One Rust owner; no Java static caches | Deterministic teardown and leak tests |

Keep only the session path required by an imported `.exe`: X server/display
bridge, one renderer path, input controls, audio, winhandler, PRoot/Wine and the
selected container/runtime components.

### S0 — Current cleanup (before migration)

- One launcher icon; first-run runtime download is user-triggered or starts only
  when launching a game.
- Fix Android back/gamepad back, permission refresh, and the unfocused-window
  freeze.
- Record the current launcher and session PSS/RSS before deleting anything.
- Add a feature checklist that proves a plain `.exe` session does not load
  Steam, GOG, stores, XR, LSFG or GPU-lab services.

### S1 — Slim session build

- Create a `session-slim` build flavor/source set rather than deleting code
  blindly.
- Remove optional native targets from the game path first; remove Java/Kotlin
  UI/services only after their references are stubbed.
- Start `XServerDisplayActivity` in `:session` and kill that process after
  normal exit (or let Android reclaim it without the launcher being affected).
- Re-measure PSS/RSS, FD and thread counts for a 10-minute `.exe` session.

Exit: a plain game launch loads only the required session libraries/services
and the session process returns to the launcher without retaining its heap.

## 4. Target architecture
```
┌──────────────────────────────┐  Intent/Binder (small)  ┌────────────────────────────┐
│ :app Rust egui launcher      │ ──────────────────────► │ :session process           │
│ library, downloads, settings │                         │ Rust SessionHost           │
└──────────────────────────────┘ ◄────────────────────── │  ├─ Runtime/Container      │
                                                        │  ├─ PRoot/Wine supervisor   │
                                                        │  ├─ Input/audio ownership   │
                                                        │  ├─ C ABI: X11/render/PRoot│
                                                        │  └─ SessionGuard drop      │
                                                        └────────────────────────────┘
                                                                  │
                                                       minimal Java Activity only for
                                                       Surface, Intent, permissions
```

Rules:

- Rust owns `SessionRequest`, `SessionId`, `SessionState`, container layout,
  environment, child process group, input state, audio backend, and teardown.
- Java provides only Android surface/lifecycle plumbing. No Java business
  state and no long-lived static session registries.
- Reused C++ libraries expose a C ABI with opaque handles and explicit
  create/destroy calls. Rust stores them in RAII wrappers and never passes
  `jobject` through the native graph.
- One session process at a time. `SessionGuard` has no `Clone`; the supervisor
  kills the process group and joins a dedicated exit thread.

## 5. Phased migration

### R0 — Baseline and leak harness (1 week)

- Record current PSS/RSS, FD count, thread count, JNI local/global reference
  counts, and heapprofd traces at: app idle, session start, gameplay, exit.
- Add a 50-cycle launch/exit stress script and an exit assertion: no child
  process, no `wineserver`, no PulseAudio PID, no surface/renderer objects.
- Add host ASan/LSan runs for Rust crates and native C ABI tests.

Exit: repeatable baseline with per-cycle deltas, not a single snapshot.

### R1 — Session contract and process boundary (2 weeks)

- Define `SessionRequest` (game, exe, container, runtime paths, settings),
  `SessionState`, `SessionError`, and a small Intent/Binder protocol.
- Move the session host to `android:process=":session"`.
- Add `gamehubx-session-core` crate and a minimal `SessionHost`.
- Keep the existing `XServerDisplayActivity` only as a temporary surface host.
- Add native wake/lifecycle signalling and a hard stop path.

Exit: start/stop a dummy session 50 times with stable FD/thread/PSS counts.

### R2 — Runtime, imagefs, containers, contents in Rust (2–3 weeks)

- Move asset bootstrap/download ownership into Rust:
  - imagefs/Proton download and checksum verification
  - session asset archive extraction and markers
  - bounded download queue and progress
- Port `ImageFsInstaller`, container metadata, component merge/removal, DXVK
  application, and marker checks to Rust. Reuse `gamehubx-runtime` and
  `gamehubx-download` where possible.
- Remove Java `ImageFsInstaller`, `ContainerManager`, `ContentsManager`, and
  their executors from the launch path.

Exit: install, repair, launch, and delete one container from Rust; restart
persistence; no Java content manager in the session path.

### R3 — PRoot/Wine process ownership (1–2 weeks)

- Replace `ProgramLauncher`/`WineHelper`/`ProcessHelper` launch logic with
  `gamehubx-supervisor` and `SessionPlan`.
- Rust owns: PRoot argv, binds, Wine env, process group, RLIMIT_AS, affinity,
  stdout/stderr readers (bounded), exit wait, and `wineserver -k`.
- No Java `Runtime.exec` or unjoined `Thread`/`Executor` remains.

Exit: 50 launch/exit cycles with no surviving `proot`, `wine`, `wineserver`,
or reader threads; exit code and PSS captured each cycle.

### R4 — Audio ownership (1–2 weeks)

- Move Pulse/AAudio lifecycle behind a Rust audio handle or C ABI.
- Own `default.pa`, sink creation, Pulse process group, and teardown.
- Remove Java audio managers and their handlers from the session path.
- Bound audio buffers; no unbounded queues.

Exit: start/stop 50 sessions; no Pulse PID, FD, or thread growth.

### R5 — Input and controls (2–3 weeks)

- Reuse `gamehubx-input` for profile parsing, mapping, and hit tests.
- Route touch/key/gamepad from a thin Java input surface to Rust via a bounded
  event channel; no Java input-control state.
- Keep X11 event injection behind a narrow C ABI to `ludashix11`/`winlator`.
- Remove Java `InputControlsManager`, `winhandler` key paths, and static key
  dispatch listeners.

Exit: controller/touch tests with no retained listener or event queue growth.

### R6 — Display bridge (2–4 weeks, optional Rust port later)

- Integrate existing C++ `ludashix11` and `vulkan_renderer` through an opaque C
  handle: create, attach surface, present, resize, detach, destroy.
- Rust owns the Android surface lifecycle and calls destroy exactly once on
  every path (normal exit, error, rotation, process kill).
- Do not port the ~100k-line X11/render stack until the C handle is stable and
  leak tests pass.
- A future `gamehubx-x11` rewrite is a separate, evidence-driven project.

Exit: display smoke test plus 50 surface create/destroy cycles with stable
native allocation counts.

### R7 — Remove the temporary Java session path (1 week)

- Delete unused Winlator Java/Kotlin UI, containers, contents, input-control
  managers, store integrations, and static listener registries.
- Keep licenses/notices and only the thin surface host plus C ABI wrappers.
- Final device leak gate: 200 launch/exit cycles, heapprofd, FD/thread/PSS
  deltas within budget; no new global JNI references.

Exit: no Java session state on the launch path, no monotonic resource growth.

## 6. Memory/leak rules for the Rust core

- One owner: `Session` owns all handles; `SessionGuard` is non-cloneable.
- No global mutable `Session`, `Context`, `Activity`, `Surface`, or renderer.
- JNI: use scoped local frames; global references are explicit tokens with
  `Drop`; never retain `JNIEnv`, `jobject`, or `GlobalRef` beyond the session.
- Threads: named, joined or cancel-signalled during `Drop`; bounded channels;
  no `spawn` without an owner/join path.
- Native ABI: opaque `Box` handles, destructor idempotent, callbacks carry a
  generation id and are rejected after detach.
- Caches: bounded (icons, profiles, fonts); clear on session end.
- Process isolation: kill the whole session process group and verify no child
  PIDs/FDs survive.
- Logging: bounded ring buffer, no listener registration per frame.

## 7. Verification matrix

| Gate | Measurement | Pass |
|---|---|---|
| Host | ASan/LSan, Rust tests, C ABI destroy tests | no leak reports |
| Device idle | PSS/RSS, FDs, threads | stable across 30 min |
| Session cycles | 50/200 launch-exit cycles | deltas bounded, no surviving PIDs |
| JNI | local/global reference counts | no per-cycle growth |
| Display | surface create/destroy | each surface destroyed once |
| Audio/input | Pulse/input handles | no PID/FD/thread growth |
| Failure paths | missing runtime, killed process, rotation, low memory | cleanup still completes |

Suggested initial budget: no monotonic PSS growth greater than 16 MB over 50
cycles, zero extra FDs/threads after exit, and no surviving session child
processes. Tighten after the first measured baseline.

## 8. Risks and decisions

- X11/render rewrite is high risk and not required for leak-free ownership.
  Reuse C++ behind a C ABI first.
- A dedicated `:session` process contains leaks/crashes but adds IPC and a
  second process's memory; it is the recommended first step.
- Android surface/IME/permission APIs still require a thin Java host.
- Winlator GPLv3 notices and source publication must remain.
- No claim of zero leaks until repeated device measurements pass; the plan
  targets measurable, bounded, per-cycle stability.
