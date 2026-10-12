# Status

The state of the Mosaïk build, with the evidence for every claim. Numbers come from runs on the
owner's PC (Windows 11 Home 25H2, build 26200, one 1920x1080 monitor at 100%, 75 Hz) or from CI,
and say which. Raw logs stay out of the repository because they contain machine details; the
relevant lines are quoted here. The probes that produce them are in `crates/mosaik-probe` and are
re-run before each release.

## Steps

| Step | State | Evidence |
| --- | --- | --- |
| 1. Verify assumptions | in progress: two monitors (virtual displays) and Windows 10 left | [Step 1](#step-1-verify-assumptions) |
| 2. Core, topologies, persistence, IPC, mosaikctl | done | [Step 2](#step-2-core-persistence-ipc) |
| 3. Zones | in progress: capture, frames and tags, drag to snap done; zone editor left | [Steps 3 and 4](#steps-3-and-4-zones-and-workspaces) |
| 4. Workspaces | in progress: save, switch, start missing apps, switcher, tray done; per-workspace hotkeys left | [Steps 3 and 4](#steps-3-and-4-zones-and-workspaces) |
| 5. Tabs | not started | |
| 6. Product UI | not started | |
| 7. Hardening | not started | |
| 8. Release | decided: Microsoft Store, direct download from mosaïk.com (ADR 0008) | |

## Step 1: verify assumptions

### The owner's PC (2026-10-11)

- Windows 11 Home 25H2 (26200.9457), x64, 16 GB RAM, 28 logical processors. Home edition: no
  Hyper-V, Windows Sandbox or Remote Desktop host, so the Windows 10 and RDP passes need another
  machine or a VM. 18.6 GB free on C:.
- One monitor: 1920x1080 at 100% (96 DPI), 75 Hz, taskbar at the bottom (work area 1920x1032).
  Window animations on.
- Keyboard layouts loaded: English (US) and French (AZERTY) in the user's list, plus two Canadian
  layouts present in the session (handles `0x0C0C` and `0x1009`).
- Rust 1.97.0 (pinned in `rust-toolchain.toml`), MSVC Build Tools 2022, Windows SDK 10.0.26100,
  Node 24.6, WebView2 runtime 154.

### Window classification (`mosaik-probe classify`, 2026-10-11)

```
326 top-level windows; full scan 9.9 ms (median of 10; max 54 ms)
  manage 7 · pending:minimized 2 · ignore: hidden 303, cloaked 10, shell 2, popup 1, tool_window 1
```

The full-screen game (`UnrealWindow`, a popup) and the shell's own windows are ignored; Chrome,
VS Code, Windows Terminal, Explorer, mintty and two other Chromium-based apps are managed. Titles are not
recorded, only their length.

### Placement (`mosaik-probe matrix`, 2026-10-11) — ADR 0005

Eight apps (Notepad, Edge, Chrome, VS Code, Windows Terminal, Explorer, Word, Calculator) placed
on four targets each, plus "restore from maximised into the centre":

```
placement error px (after settling): max 0.0 on every app (40 placements)
  Chrome/Edge  narrow 420 wide   constrained true  wanted 1410..1912 (502 px), frame 1410..1912
  Terminal     narrow 420 wide   constrained true  wanted 1446..1912 (466 px), frame 1446..1912
  from maximised: moved 0 drift 0 corrections 0 final 0 (all eight)
```

Before the fixes in ADR 0005 the same probe found: Chrome restored into the left half 7 px inside
it (an asynchronous restore landing last), VS Code moving itself 982 px back to its previous
place after a restore, and Chrome, Edge and Terminal hanging 82, 82 and 46 px off screen in a
420 px zone.

### Hiding background tabs (`mosaik-probe matrix`, 2026-10-11) — ADR 0003

With a magenta backdrop under the app, so captures show the app or the backdrop, never the
user's desktop:

```
                hidden (backdrop)   hide      show      first frame drawn
  Shell cloak   1.0 on 8/8 apps     1-8 ms    1-3 ms    1-3 ms (Calculator 213 ms after an earlier minimise)
  Minimise      1.0 on 8/8          1-16 ms   1-32 ms   203-217 ms (restore animation)
  Park          1.0 on 8/8          1-2 ms    1-2 ms    ~1 ms
  DWM cloak on another app's window: refused on 8/8
  Chrome/Edge animated page: 5 changes/s before hiding and after every uncloak
  maximised windows stay maximised with all three methods
```

An earlier run without the backdrop (and with blank pages) compared captures with whatever was
behind the app and showed large differences for Chrome and Edge after an uncloak. With the
backdrop and an animated page, both draw at once and keep drawing, so that run is not used.

### Zone preview latency (`mosaik-probe drag`, 2026-10-11)

A window in another process dragged by synthetic input across three zones, the preview
overlay (a layered window) updated on every move, which is the worst case:

```
250 moves (0 missed); render 3 previews 14.5 ms (once)
  input -> WinEvent        p50 1.14  p95 2.33  p99 3.03  max 13.18 ms
  input -> overlay updated p50 1.87  p95 3.45  p99 4.66  max 14.07 ms
  input -> frame composed  p50 13.34 p95 14.68 p99 17.33 max 38.07 ms   (75 Hz: 13.3 ms a frame)
  zone changes -> composed p50 14.28 max 15.5 ms
```

The preview reaches the screen in the next frame after the window moves. Two monitors with
different DPI are still to be measured (virtual displays, owner's go-ahead needed).

### Crash recovery (`mosaik-probe watchdog`, 2026-10-11) — ADR 0006

```
smoke test: exit Some(0), log 4 line(s)
== watchdog started by the probe
all restored: true in 59 ms after the kill; journal entries left: 0
== watchdog started by the main process
all restored: true in 18 ms after the kill; journal entries left: 0
  cloak, minimize, park and a cloaked Notepad: shown, frame error 0 px
  watchdog log: process ended -> COM ready -> journal lists 4 window(s) -> done: 4 restored (65 ms)
```

### Switcher hotkey (`mosaik-probe hotkey`, 2026-10-11) — ADR 0004

`RegisterHotKey` trials (registered, then released at once):

```
Win+Alt+Space          registered (free)
Win+Ctrl+Alt+Space     registered (free)
Win+Alt+S              registered (free)
Win+Alt+Z              registered (free)
Win+Alt+W              FAILED error=1409 (already registered)
Win+Ctrl+Space         FAILED error=1409
Win+Shift+Space        FAILED error=1409
Win+Space              FAILED error=1409
```

Pressed with synthetic input, with each loaded layout active:

```
Win+Alt+Space fired: true after 2.759 ms
  English (US)        Win+LeftAlt+Space true  Win+RightAlt+Space true   RightAlt+Space fired false typed []
  French (AZERTY)     Win+LeftAlt+Space true  Win+RightAlt+Space false  RightAlt+Space fired false typed []
  layout 0C0C         Win+LeftAlt+Space true  Win+RightAlt+Space false  RightAlt+Space fired false typed []
  layout 1009         Win+LeftAlt+Space true  Win+RightAlt+Space false  RightAlt+Space fired false typed []
```

Layout tables, read from the layout DLLs without installing anything:

```
  English (US)                     AltGr false | AltGr letters/digits typing: 0
  Canadian Multilingual Standard   AltGr true  | AltGr+Space U+00A0 | right Ctrl VK 0xDF | AltGr letters/digits typing: 8
  Canadian French                  AltGr true  | AltGr letters/digits typing: 14
  French (AZERTY)                  AltGr true  | AltGr letters/digits typing: 8
  German (QWERTZ)                  AltGr true  | AltGr letters/digits typing: 9
```

### Idle cost of the engine loop (`mosaik-probe idle`, 2026-10-11)

WinEvent hooks, the hotkey registered and a reconcile scan every 2 s, for 60 s:

```
60 s: CPU 0.208% of one core (0.0074% of the machine), 31 scans, private 1.49 MB, working set 10.89 MB
```

### The whole app at idle (release build, 60 s, jobs 0019, 0025, 0026) — ADR 0002

Mosaïk started with its data in a scratch folder, tray icon only:

```
switcher as a hidden web window:   private 153.6 MB, working set 372.1 MB, 8 processes (6 WebView2), CPU 0.156%, ready 1533 ms
native switcher, hardware Direct2D: private  51.4 MB (mosaik 50.4, watchdog 1.0), CPU 0.078%, ready 400 ms
native switcher, software Direct2D: private   5.2 MB (mosaik 4.2, watchdog 1.0), working set 29.0 MB, CPU 0.000%
```

Budget from these runs: under 15 MB private for Mosaïk and its watchdog with no web window open,
under 1% CPU. The binaries import no Visual C++ runtime (static CRT; `api-ms-win-crt-*` is part
of Windows 10 and 11).

### Smart App Control (job 0024, 2026-10-11) — ADR 0008

Smart App Control is on on the owner's PC (`VerifiedAndReputablePolicyState` 1). It blocked a
new release `mosaikctl.exe` (Code Integrity events 3033 and 3077, "did not meet the Enterprise
signing level requirements") and let the new `mosaik.exe`, the watchdog and a debug
`mosaikctl.exe` run: unsigned files are judged one by one. Test jobs fall back to the debug
build; releases are signed by the Microsoft Store.

### Still to do in step 1

- Two monitors with different scale factors and hotplug, with virtual displays: the driver is
  installed (Virtual Display Driver 25.7.26 from winget, owner's go-ahead), two virtual
  monitors configured; the owner sets their scales.
- Windows 10 22H2: needs a second machine or a VM (this PC has 18.6 GB free, too little for a VM).

## Step 2: core, persistence, IPC

- `mosaik-core`: 66 tests pass on Linux and on Windows (`cargo test -p mosaik-core
  --all-features`), including property tests: ratios round-trip to the same pixels on any
  reference rectangle, shared zone edges stay shared, `fit_inside` always lands inside, zones
  made on any of 18 monitor arrangements land on a connected monitor's work area on any other,
  ratios survive DPI changes, and `fit_size` keeps any window size on screen while covering as
  much of its zone as it can.
- Bug found by a property test and fixed: `serde_json` parsed some floats one ulp off, so a zone
  saved and loaded could move by a fraction of a pixel. Fixed by enabling serde_json's
  `float_roundtrip` feature; `ratio_survives_json` covers it.
- `mosaik-ipc`: 9 protocol tests on Linux and Windows, 10 named-pipe tests on Windows (ADR 0007).
  The first version was reviewed adversarially; the review found an instance stuck after a
  client left mid-reply, squatters passing the program check, impersonation-level client handles,
  every open error read as "not running", no deadlines, an unaligned read and a rescue fallback
  that lost entries. All fixed, each with a test where it can be tested.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean on Windows
  (2026-10-11).
- The engine thread, `mosaikctl state --json` and the Tauri shell run on the owner's PC
  (job 0026): ready in 0.4 s once Windows knows the build, `state` lists monitors, DPI, windows
  and rectangles, and for each window of the open workspace its slot and zone.

## Steps 3 and 4: zones and workspaces

- `mosaik-core` 86 tests (Linux and Windows): capture on all 18 fixtures puts every zoned window
  on a connected monitor's work area; tiled windows become zones, stacked ones share one,
  floating and maximised ones keep their place; saving again keeps zones; switching moves,
  starts, minimises (only windows on screen), raises and focuses; refitting after a display
  change; one membership rule (`holds`); titles tell windows of one app apart; WPF and WinForms
  class names that change every run still match.
- First run of `mosaik-probe workspace` (job 0026, three probe windows, engine in process):

  ```
  saved: 3 windows, 3 zones (77 ms); windows inside their zones: error 0 px on all three
  tags in the zone colour (exact); rings drawn but darkened by each window's own shadow
  switch back after scrambling: 20 ms, but two windows stayed where they were scrambled
  ```

  The switch had first "remembered" the scrambled places. Fixed (layouts now change only by the
  user's hand), along with eleven findings of an adversarial review of the same change: moves
  that could block the engine on a hung app (now on threads, 2.5 s limit), minimising hidden
  and other-desktop windows, refitting hidden windows, exact class matching, WM_CLOSE
  destroying the engine window, inconsistent zone membership, name comparisons, the drop
  target's bitmap (now a solid tint), and drag details. Frames now sit right above their window.
  Re-run pending.

## Owner decisions

- 2026-10-11: the Patreon link is https://www.patreon.com/gimzdev (About and the tray menu only).
- 2026-10-11: one click from mosaïk.com, minimal and free; distribution through the Microsoft
  Store's direct web installer (ADR 0008). The owner registers the Store developer account and
  verifies their identity; publishing each release stays their action.
- 2026-10-11: Virtual Display Driver installed for the multi-monitor and mixed-DPI tests.
