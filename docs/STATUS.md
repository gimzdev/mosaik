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
| 3. Zones | done: capture, frames and tags, drag to snap, zone editor | [Steps 3 and 4](#steps-3-and-4-zones-and-workspaces), [Step 6](#step-6-product-ui) |
| 4. Workspaces | done: save, switch, start missing apps, switcher, tray, workspace shortcuts | [Steps 3 and 4](#steps-3-and-4-zones-and-workspaces) |
| 5. Tabs | done, with 12 real apps and browser tabs dragged out of Edge; multi-monitor left for step 7 | [Step 5](#step-5-tabs), [Step 7](#step-7-hardening) |
| 6. Product UI | in progress: every page done, reviewed and tested in a browser; embedded and run on Windows; the on-screen check of every page on Windows is next | [Step 6](#step-6-product-ui) |
| 7. Hardening | in progress: real apps, Windows' snapping and other arrangers done; sleep, hotplug, RDP, soak and Windows 10 left | [Step 7](#step-7-hardening) |
| 8. Release | Store package, licences, one-file app and site ready; Store submission is the owner's | [Step 8](#step-8-release) |

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
- Re-run with the fixes (job 0031, release build): save 64 ms and switch back after scrambling
  48 ms for three windows, every window 0 px from its place after both, ring and tag colours
  exact; a closed window started again by the next switch, 0 px from its place.
- Workspaces' own shortcuts: registered from `WM_HOTKEY` id 100 on; every shortcut is checked
  against the AltGr characters of the installed keyboard layouts (`mosaik_win::keyboard`), the
  rules of `mosaik_core::hotkey::check`, Mosaïk's own shortcuts, and a live registration probe
  for other apps.

## Step 5: tabs

- Design and the rule for what shows: architecture.md "How tabs behave", ADR 0009.
- An adversarial review of the first version (three reviewers: engine, overlay, core) found 35
  issues, the worst being windows that could stay hidden for good. All fixed in d508938: one
  reconcile from `mosaik_core::tabs::plan_tabs` (unit tests), hiding on its own thread with a
  deadline, journal written once per batch with the marker set before hiding, strips in the
  zone's band (always reserved for groups), chip layout with a "+N" chip (`mosaik_core::strip`,
  property test: everything drawn is inside the strip and clickable), tabs identified by window
  rather than position, damaged groups mended on load (a self-referencing group overflowed the
  stack before), a property test over random tab operations (512 runs of 1 to 40 operations).
- With the real mouse on Windows (job 0031, `mosaik-probe workspace --drag`, release build):

  ```
  drop the terminal on the code window's tag: 1 strip, 2 tabs, terminal 0 px from the group's
    place, code hidden
  click the code tab: code shows, 0 px; terminal hidden
  pull the terminal's tab out into the review zone: no strip left, terminal 0 px from the zone,
    code shows
  drag the terminal back by its title bar: 0 px, ring in the zone's colour
  probe process private memory with frames and strips: 6.9 MB
  ```

## Step 6: product UI

- One window (built when opened, destroyed when closed): welcome on the first run, workspaces
  (open, zones, shortcut, rename, delete with confirmation), the zone editor (drag or arrow-key
  the lines between zones, split, rename, recolour, remove, presets; nothing changes until
  saved), settings (shortcut recorder that refuses AltGr, start with Windows, theme, snapping,
  frames, tags, switching, tabs), about (privacy, Patreon, website, source, rescue, logs).
- Interface checks (Linux and Windows): typecheck, ESLint (strict, type-checked), Prettier, 39
  component tests, and 9 browser smoke tests on the development server's stand-in engine, each
  with axe-core's WCAG 2.1 A and AA rules: no serious or critical violation in the light, dark
  or high-contrast theme, nor at the smallest window size; the keyboard reaches every page; a
  zone line moves with the arrow keys; the welcome fits the window; when the engine does not
  answer, the window says so and offers to try again. The first axe run found shortcut keys
  labelled in a way screen readers ignore, fixed.
- Adversarial review (2026-10-12) of the engine commands (18 findings) and the interface (24),
  all fixed with tests: a zone editor opened before the monitors or zones changed can no longer
  save over them (its save carries a revision); the shortcut recorder holds Mosaïk's own
  shortcuts while it listens; messages survive moving between pages; high-contrast mode keeps
  chips, swatches and disabled buttons visible; and more (commits bfdb563, 93487ca).
- On Windows (jobs 0033 to 0038, release build): the interface is embedded in `mosaik.exe` (a
  compile-time check refuses a build that would load it from a development server). First run:
  `engine ready ... hotkey=Win+Alt+Space registered=true`; the welcome opens. With the settings
  window open: 8 processes, 165.7 MB private, nearly all of it WebView2 (the window is destroyed
  when closed; the idle budget is measured with no window open).
- The editing commands, through the engine (`mosaik-probe workspace`, job 0036): splitting the
  code zone moved its window to the new half, 0 px off, in 47 ms; the editor opened before was
  refused; removing the terminal's zone left the terminal where it was (0 px) and in no zone;
  after a rename the old name saved a new workspace; Win+Ctrl+Alt+F9 given and taken back;
  deleting the open workspace left no workspace open and no window hidden.

## Step 7: hardening

### Real apps with the real mouse (`mosaik-probe workspace --apps`, jobs 0047 to 0051)

Each app opens a new window of its own (browsers in private mode); the engine's scope lists the
probe's windows one by one, so the user's own windows of the same apps are never managed. The
window is grabbed where it answers `HTCAPTION` in the middle of its title bar, dragged into a
zone, then onto another window's tag; the other tab is clicked, then its own; then its tab is
pulled back out into the zone. Windows' move loop and mouse capture are recorded for each drag.

```
                     into the zone   onto a tag   other tab clicked   own tab clicked   pulled out
Notepad                  0 px         2 tabs, 0    hidden (cloaked)    shown, 0 px       0 px
Edge                     0 px         2 tabs, 0    hidden (cloaked)    shown, 0 px       0 px
Chrome                   0 px         2 tabs, 0    hidden (cloaked)    shown, 0 px       0 px
VS Code                  0 px         2 tabs, 0    hidden (cloaked)    shown, 0 px       0 px
Windows Terminal         0 px         2 tabs, 0    hidden (cloaked)    shown, 0 px       0 px
Explorer                 0 px         2 tabs, 0    hidden (cloaked)    shown, 0 px       0 px
Word                     0 px         2 tabs, 0    hidden (cloaked)    shown, 0 px       0 px
Calculator (UWP)        22 px*        2 tabs, 0    hidden (cloaked)    shown, 0 px      22 px*
Notepad++                0 px         2 tabs, 0    hidden (cloaked)    shown, 0 px       0 px
Paint                   13 px*        2 tabs, 0    hidden (cloaked)    shown, 0 px      13 px*
Excel                    0 px         2 tabs, 0    hidden (cloaked)    shown, 0 px       0 px
Git Bash (mintty)        0 px         2 tabs, 0    hidden (cloaked)    shown, 0 px       0 px
Firefox                  not installed on this PC
```

\* Calculator and Paint do not shrink below their minimum height (the zone leaves 480 px); they
stay inside the zone as far as they can, as ADR 0005 places every window.

Found and fixed on the way:

- **A quick drag up out of a title bar was taken for a resize** (job 0047: no tab for 8 of 12
  apps; dropped at the screen's top edge, Windows maximised the window instead). Windows starts
  its move only once the pointer moves, and Mosaïk read where the pointer was when it noticed
  the start, by then above the window. The pointer's shape now decides: Windows shows its resize
  arrows for the whole of any resize (`snap::DragKind`, ADR 0010). Job 0048: every drag a move,
  and the tags take the windows.
- **Chromium-based apps** (Edge, Chrome, VS Code) start Windows' move themselves once they see
  the pointer move inside their window, and treat a drag that starts along their top few pixels
  as a resize (job 0049: the engine saw Windows' resize arrows from the start). The probe now
  grabs title bars in their middle and speeds up from rest, as a hand does (job 0050: all three
  end to end).
- **A browser window closed at the end of its drag** (a tab dragged back into its window) no
  longer brings a "did not move into its zone" notice.

### Windows' own snapping (`mosaik-probe workspace --drag`, jobs 0048 and 0051)

Windows 11 25H2, "Snap windows" on:

```
onto a tab strip at the screen's top edge (576,0):   became a tab, not maximised, 0 px from the group's place
onto a tab strip at the top centre (960,0):          became a tab, not maximised, 0 px
into a zone at the left edge (0,516):                in the zone, 0 px; Windows marks the window arranged
notice (once, at the first of these drops):          "Windows snaps windows too: ..."
```

Mosaïk acts after Windows at the end of the drag, so the place the user dropped on wins;
Windows' own preview still shows during the drag, hence the notice (ADR 0010). Mosaïk never
changes Windows' setting.

### Other arrangers

komorebi, GlazeWM, workspacer, FancyWM, PowerToys FancyZones, AquaSnap and WindowTabs are found
by their program in one process-list snapshot (when Mosaïk starts and after each switch) and
named once, ever, with what to do (`mosaik_core::others`, unit tests for finding, wording and
keys). None of them is installed on this PC, so the notice itself is checked by the unit tests
and the probe's notice for Windows' snapping, which goes through the same path.

### Browser tabs dragged out (`mosaik-probe workspace --browser`, jobs 0046 to 0056)

A browser window with three tabs (the probe's own pages); tab 2 is dragged out onto the review
zone, then tab 3 onto the code window's tag. The user's browser is never touched: Chrome runs as
a separate copy with a profile of its own in the temp folder, Edge in a private window, and the
engine manages only windows showing the probe's pages.

```
Edge (job 0046):    onto the zone: a new window, 0 px from the zone, part of the workspace
                    onto the tag:  a new window, a strip of two tabs with the code window
Chrome (job 0055):  onto the zone: a new window, 0 px from the zone, part of the workspace
                    onto the tag:  a new window, a strip of two tabs with the code window
```

Chromium tears a tab off through Windows' move loop, so a tab being torn off is a window being
dragged: zones and tags take it like any other. On its way up to the tag, the tab passed over
its old window's tab strip; Chrome put it back there and tore it off again, and the window it
closed meanwhile was let go quietly (the fix of job 0046). Firefox tears a tab off only when it
is dropped, so there is no window to follow during the drag: its new window is dragged into a
zone or onto a tag afterwards, like any window (not installed on this PC).

### Soak (`mosaik-probe workspace --soak 300`, job 0055)

300 switches between two workspaces of the three probe windows (A tiled, with the terminal a
tab of the code window, so each switch to A hides a tab and draws a strip; B in three columns),
in 1.4 minutes, measured in the probe's process where the engine and its overlays run (a debug
build: Smart App Control blocked that release build of the probe, job 0053):

```
                    after 10 switches   after 300   change
switches failed                                  0
GDI objects                     68           68        0
USER objects                    52           52        0
handles                        343          341       -2
private memory              11.5 MB      15.7 MB    +4.2 MB, flat from switch 180 on
switch time (mean)           32.5 ms      33.1 ms
```

## Step 8: release

- **Store package** (`installer/msix/package.ps1`; `mosaik.ps1 package`): makepri indexes the
  logos at every size, makeappx packs (job 0038): `Mosaik_0.1.0.0_x64.msix`, 2.6 MB, 30 files:
  `mosaik.exe` 4.3 MB, `mosaik-watchdog.exe` 0.35 MB, `mosaikctl.exe` 0.59 MB,
  `THIRD-PARTY-NOTICES.txt`, `LICENSE.txt` (CC0 1.0 legal code), 21 logos. The first attempt
  failed makeappx's manifest validation (logos named in one size only), fixed. Not yet installed
  from the package: that needs the Store, or Developer Mode on this PC (off).
- **Licences**: `THIRD-PARTY-NOTICES.txt`, made by `installer/notices.mjs` from what ships: 134
  crates (as `cargo tree` resolves them for Windows x64, without build-only, test-only and
  procedural-macro crates) and 10 packages and fonts of the settings window. The same file is
  made on Linux and on Windows; CI fails when it is out of date. About opens it.
- **The one-file app**: `mosaik.ps1 pack`, `verify`, `unpack`, `verify`, `check` (job 0035),
  then `build` (120 s), `test` (every test, clippy and interface check passed) and `package` (job
  0038), all with Windows PowerShell 5.1 and output redirected as in CI. The first `build` run
  stopped at npm's first warning (Windows PowerShell 5.1 turns a program's stderr into errors
  when output is redirected), fixed.
- **Site** (`mosaik1.sh`): one-click download through the Store's web installer
  (`get.microsoft.com/installer/download/9PHR0L7S1QVD`) in the header, the hero and the closing
  call to action; `/download` with requirements, the welcome's three steps and what is inside;
  the switcher illustration drawn like the app's switcher (Win Alt Space, 1 to 4, S); the demo
  desktop like Windows (no top bar, the taskbar stays); Support on Patreon; `/conditions` as
  shipped (no network requests from the app, updates through the Store). `bash mosaik1.sh
  --build`: 7 static routes, type-checked; checked at desktop and phone widths; shellcheck clean;
  `npm audit`: 0 vulnerabilities; a fresh build from a copy with Windows line endings works.
  Deploy only once the Store listing is live (the Download buttons install from it).

## Owner decisions

- 2026-10-11: the Patreon link is https://www.patreon.com/gimzdev (About and the tray menu only).
- 2026-10-11: one click from mosaïk.com, minimal and free; distribution through the Microsoft
  Store's direct web installer (ADR 0008). The owner registers the Store developer account and
  verifies their identity; publishing each release stays their action.
- 2026-10-11: Virtual Display Driver installed for the multi-monitor and mixed-DPI tests.
- 2026-10-11: Store developer account "Gimzdev" registered; the name "Mosaïk" is reserved
  (Store ID 9PHR0L7S1QVD, identity in `installer/msix/identity.json`). The first submission is
  due by 2027-01-11 or the name is released.
- 2026-10-11: everything travels in two files that run on another machine: `mosaik.ps1` (the app,
  source inside) and `mosaik1.sh` (the site).
- 2026-10-11: `mosaik1.sh` reviewed end to end, optimised and cleaned as the final version.
