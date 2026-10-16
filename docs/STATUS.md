# Status

The state of the Mosaïk build, with the evidence for every claim. Numbers come from runs on the
owner's PC (Windows 11 Home 25H2, build 26200, one 1920x1080 monitor at 100%, 75 Hz) or from CI,
and say which. Raw logs stay out of the repository because they contain machine details; the
relevant lines are quoted here. The probes that produce them are in `crates/mosaik-probe` and are
re-run before each release.

## Steps

| Step | State | Evidence |
| --- | --- | --- |
| 1. Verify assumptions | done: two monitors at one scale (job 0072) and at two (100% and 125%, job 0116); Windows 10 cannot be run here (no second PC or VM) | [Step 1](#step-1-verify-assumptions) |
| 2. Core, topologies, persistence, IPC, mosaikctl | done | [Step 2](#step-2-core-persistence-ipc) |
| 3. Zones | done: capture, frames and tags in title bars, drag to snap, zone editor | [Steps 3 and 4](#steps-3-and-4-zones-and-workspaces), [Step 6](#step-6-product-ui) |
| 4. Workspaces | done: save, switch, start missing apps, switcher, tray, workspace shortcuts | [Steps 3 and 4](#steps-3-and-4-zones-and-workspaces) |
| 5. Tabs | done, with 12 real apps and browser tabs dragged out of Edge; multi-monitor left for step 7 | [Step 5](#step-5-tabs), [Step 7](#step-7-hardening) |
| 6. Product UI | done: every page reviewed, tested in a browser and toured on Windows (names, Tab key, light and dark, smallest size); a Windows contrast theme and Narrator by ear are in the release checks | [Step 6](#step-6-product-ui) |
| 7. Hardening | done here: real apps, every kind of app window (borderless, fixed size, full screen), tags in title bars, Windows' snapping, other arrangers, soak, two screens at two scales; left for the owner by hand: sleep and resume, a screen unplugged; not possible here: RDP (Windows 11 Home has no Remote Desktop host), Windows 10 | [Step 7](#step-7-hardening) |
| 8. Release | Store package, licences, one-file app and site ready (the download is the Store's installer, the owner's choice); Store submission is the owner's | [Step 8](#step-8-release) |
| 1.0.1, first builds | (called 1.1 then) built, tested and packaged on Windows (jobs 0094 to 0118: every test, the Store package); zones that follow a resize, Arrange laid out for the task with three windows to a view, zones that make room for a window too big for them, and its undo, the home page with every workspace and its ⋯ menu, Settings with More settings and About, a workspace's apps, windows edge to edge without rings, tab strips in one place and back above after a click, no window moved when brought forward, the quick menu where the pointer stops and never for a game, all measured with the mouse; sets of apps read from the owner's Start menu, every app started once and placed as soon as it shows; opening a set on the owner's screen not run by a job (it starts apps) | [1.0.1: first builds](#101-first-builds) |
| 1.0.1 | (called 2.0 until 2026-10-13) built, tested and packaged on Windows (jobs 0122 to 0143): the window rebuilt around the screen and Arrange, dark by default, apps by their own icons (11 of 11 in the suggested sets), toasts instead of alerts, the side menu, windows dragged onto each other in Home's picture; every test and every probe check passing; the Store package 1.0.1.0 ready; `./mosaik1.sh --deploy --app` tests, builds, packages and sends it to the Store with nothing to answer once the Store's key is kept (the key alone asked on the first run, ADR 0026), then deploys the site; the owner's Microsoft Entra tenant made, linked to the developer account, and its application "Mosaik Store publishing" made with a key on 2026-10-13 | [1.0.1: the window rebuilt](#101-the-window-rebuilt) |

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

Each build is judged again, and a verdict mostly sticks to the file: in jobs 0079 to 0081 it
blocked six test programs, then the `mosaik-core` and `mosaikctl` test programs of one commit
four times out of four, a minute apart ("An Application Control policy has blocked this file",
the tests never ran), while `mosaik-win`'s ran on a second try. Jobs now run each package's
tests on their own and try a blocked program again; a test program blocked for good is
reported as such, never as passing, and its tests are counted from the last run that executed
them (on Windows, or on Linux for `mosaik-core`). Job 0084 had every test program blocked (the
`mosaik-win`, overlay and engine ones, three tries each) while the debug probe and the app ran:
the Windows tests last ran at commit b993011 (job 0082: all passing), and what changed since in
`mosaik-win` and the engine is the band drag, which the probe drove with the real mouse. CI on
`windows-latest` has no Smart App Control and runs every test.

It refuses libraries a build makes too: in jobs 0092 and 0093 the engine's tests would not build
("can't find crate for `tracing_attributes`", three tries each). Job 0094 read why from the Code
Integrity log: events 3033 and 3077 at 16:04, rustc refused the debug build of that procedural
macro's DLL ("did not meet the Enterprise signing level requirements"). The DLL made again, the
same size, was let through, and the engine's tests ran (2 passing). `mosaik.ps1 build` and `test`
now do the same on their own: when cargo fails and the log names files of the build it refused,
they are removed, said, and cargo runs once more (`Invoke-Cargo`). Job 0097 saw it happen: Smart
App Control refused a new `mosaikctl` test program ("error: 1 target failed"); `mosaik.ps1 test`
said "Windows (Smart App Control) refused 1 file(s) this build made:
mosaikctl-fc4207af53db9f1f.exe. Making them again.", and every test then passed (core 185).

### Left in step 1

- Two monitors with different scale factors, with virtual displays (Virtual Display Driver
  25.7.26 from winget, owner's go-ahead): done in job 0116, the main screen at 100% and a
  virtual 2560x1440 one at 125% (`workspace --monitors`, "1.0.1: first builds" below). A screen unplugged and
  plugged back (switching the virtual displays off and on changes Windows' display settings):
  left for the owner, by hand.
- Windows 10 22H2: needs a second machine or a VM (this PC has 28 GB free, too little for a VM);
  Windows 10 has been out of support since 2025-10-14.

## Step 2: core, persistence, IPC

- Windows tests with real windows the test makes (`mosaik-win/tests/windows.rs`, run by
  `mosaik.ps1 test` and CI; job 0074: all pass): a window placed in four places lands on each
  with its visible frame 0 px off; a maximised window is restored into its place; its title bar
  is read as real input finds it (room for a tag, clear of the buttons Windows draws; caption
  where the reading says); the scanner and classifier take it for an app window; it is hidden
  and shown back in place by each hiding method (shell cloak, minimise, park); a window without
  a frame to resize it by keeps its size in a larger and a smaller place, and after a restore
  from minimised (job 0078: failed on the code before the fix, then all 7 pass).

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
- Interface checks (Linux and Windows): typecheck, ESLint (strict, type-checked), Prettier, 44
  component tests, and 10 browser smoke tests on the development server's stand-in engine, each
  with axe-core's WCAG 2.1 A and AA rules: no serious or critical violation in the light, dark
  or high-contrast theme, nor at the smallest window size; the keyboard reaches every page; a
  zone line moves with the arrow keys; the Tab key goes round the window and never leaves it;
  the welcome fits the window; when the engine does not
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
- Mosaïk's own window on Windows, page by page (`mosaik-probe ui-tour`, jobs 0070 and 0071,
  release build, a profile made for the tour: one workspace with three zones, no windows, first
  run): welcome, workspaces, zone editor, settings, about and the switcher, in the theme Windows
  uses, in light and in dark, and at the smallest size the window can be dragged to (762 x 592
  with its title bar). Every control UI Automation exposes has a name (what Narrator reads): 0
  unnamed on every page. The Tab key reaches every control of every page and comes back round:
  welcome 2 stops, workspaces 7, zone editor 12 (the lines between zones included, by name),
  settings 14 (a radio group is one stop), about 11. The switcher shows the workspace with a
  preview of its zones and closes with Esc, in both themes.
- Found and fixed by the tour: after a page's last control, Tab left the page for an unnamed
  stop with nothing to see (job 0070), now it goes round (`focusLoop.ts`, with a component test
  and a browser test); a chosen radio showed two focus rings, now one.
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

### Every app in the Start menu (`--installed`, jobs 0061 and 0083)

Every app of the owner's Start menu that the probe's rules let through, each program once per
run, with the real mouse through the same steps as above (into a zone, onto a tag, both tabs
clicked, pulled out; dragged by its band when it wears one), then closed. Left out: installers
and updaters, documents and links, system and administrator tools, accessibility, camera and
phone apps, apps that sign in to an account (mail, chat, music, game launchers), hardware
tools, and entries that start a program already tested (`apps::skip_reason`). Job 0083, this
build: 142 entries, 33 let through, 22 programs, every step of every one passing:

```
0 px at every step     Access, Acrobat, Windows Terminal, Excel, File Explorer, Git Bash, Git GUI,
                       IDLE, Edge, Notepad++, PowerPoint, Publisher, VS Code, Windows Fax and Scan,
                       Windows Media Player Legacy, PowerShell ISE, Word
its own size kept      Character Map (a dialog of a fixed size, 477 x 430: in the middle of its
                       zone and of the group's place, tag in its title bar; not managed before)
                       Calculator (its minimum height), Chrome (856 x 652, its first window),
                       Photos (1 px taller): each 0 px from where that size fits best
by its band            Calculator, into the editor's zone: 0 px
```

Job 0061, before tags moved into title bars: 144 entries, 34 let through, all passing; Dev Home
was in the Start menu then, and Character Map was not managed.

### Apps that drag their own window (job 0059)

A window whose app moves it itself as the pointer moves, with no move loop of Windows' (the
probe's own window with `--self-move`, as some custom title bars do): dragged by a press in it
into the zone, 0 px from the zone, in the workspace; onto the code window's tag, a strip of two
tabs, 0 px; its tab pulled out into the zone, 0 px. Windows reported mouse capture and no move
loop for each drag (the engine's log: "the app drags its window itself").

### Every kind of app (jobs 0077 and 0078) — ADR 0012

The probe's own window stands in for the kinds of apps the catalog has none of (`--only kinds`),
each dragged with the real mouse into a zone, onto the editor's tag, shown by its tab, and pulled
out again (1920x1080 at 100%):

```
                                       into the zone                 made a tab, shown         pulled out
borderless app, own 32 px title bar    0 px, tag in its title bar    2 tabs, 0 px              0 px
game in a fixed window                 626x413 kept where 856x508    2 tabs, in the middle     kept, 0 px
                                       was given, middle, 0 px       of the group, own size
borderless game of a fixed size        640x420 kept, middle, 0 px    the same                  kept, 0 px
borderless fixed-size game that        560x360 kept, middle, 0 px,   2 tabs, in the middle     0 px
  moves itself, nothing to grab        a solid band right above it   below the band
game in full screen                    left alone ("pending: full screen"); the workspace saved
                                       around it took the 3 other windows; its frame untouched
```

"0 px" for a window that kept its size is its distance from the middle of its place
(`fit_size`); 306, 302 and 318 px from the group's edges are what the middle is for those sizes
(job 0080 measured it directly: kept, 0 px from the middle of the group's place).

Dragged by its band (jobs 0082 and 0084, after the reviews below): the borderless fixed-size
game that moves itself, pressed on its band and dragged into the editor's zone, landed there at
its own size, in the middle, 0 px, below a band again; Calculator, by its band, into the
editor's zone, 0 px; the game made a tab of the editor's group, the group dragged by its strip's
band beside the chips into the other zone and back: both tabs went along, the game at its own
size in the middle, 0 px each way. The window follows the pointer, moved by Mosaïk from a thread
of its own; a posted `SC_MOVE` (job 0080) had moved nothing.

- Steam's library window (job 0078): class `SDL_app` of `steamwebhelper.exe`, a popup with a
  caption, a resize frame and a minimise button, in Alt+Tab: Mosaïk manages it now (it took it
  for a splash screen before). It was open on the owner's screen, so the probe, which touches
  only windows it opens, left it alone; the borderless app above stands in for it.
- Calculator, which wears its tag above it, has a solid band inside its ring (job 0077), and
  with tags above every window the bands read #EDEFEA from the screen, the rings in each zone's
  colour around band and window together (job 0077, `--drag --tags-above --shots`).
- **Games of a fixed size were stretched** (job 0077: the probe's fixed-size game filled its
  zone): a window without a frame to resize it by is now only moved, with a Windows test that
  failed on the old code (job 0078: 626 x 473 became 1060 x 1032) and passes on the new.
- Found by a review of these changes, each fixed with a test that failed first (`mosaik-core`,
  151 tests): a fixed-size window larger than the work area was shrunk to it, and restored from
  minimised at that size (`fit_size` never changes a size now); one larger than its zone, where
  Mosaïk put it, was not counted in the zone, and one covering two zones could be counted in the
  wrong one (`holds_fixed`, and the zone a window was put in is kept); opening a workspace whose
  window was in full screen started its app again (it is the workspace's, left as it is); a
  window in full screen still wore its frame and could be moved below a band (never framed or
  moved now); a window placed over a monitor without a taskbar was taken for one in full
  screen (a window with Windows' title bar and resize frame never is); with zones that overlap,
  a window filling the larger was counted in the smaller (the nearest middle wins); the solid
  band let presses through to whatever was behind it (the window can be dragged by it now).
- Found by a second review, of the band drags (job 0084 checks them with the real mouse): a
  group dragged by its strip's band cancelled its own drag as soon as the strip hid (the strip
  now keeps the press); the window was moved on Mosaïk's own thread, so an app slow to answer
  would have frozen Mosaïk (a thread of its own moves it now); a window could be dragged by its
  band below every screen (it stays reachable); Esc also reached the app in front (Mosaïk holds
  Esc while the button is down); a cancelled drag left a "Drop to add" slot on the strip it
  crossed; a window over its whole monitor could wear its tag over its own title bar.

### Tags and tabs in title bars (jobs 0062 to 0070) — ADR 0011

(Measured then. Since ADR 0027 a tab group's strip is in a band above it, never in a title bar;
tags of windows alone stay in their title bars.)

Tags and tab strips now go in each window's own title bar, in a stretch the window answers
"caption" for, and windows fill their zones (28 px more each at 100%). The probe windows (classic
title bars, 1048 and 856 px wide):

```
after saving:   each tag in its title bar, 6 px left of the minimise button; 0 px from the zone
a tab join:     the strip (zone tag, "Probe Code", "Probe Terminal") in the visible tab's title bar
everything else of --drag as before: tabs, pull-out, snapping back, Windows' snapping, editing
--tags-above:   the band as before, 0 px everywhere (job 0062)
```

How title bars answer, along the row Mosaïk reads, asked as real input is (the window
`WindowFromPoint` finds, passing on "transparent") and as Mosaïk asks (`mosaik-probe titlebar`,
job 0064, 960 px wide windows): the two agree at every point of every app.

```
Notepad, Notepad++, Paint   caption 22 to 854, then Windows' buttons; Windows draws them from 814
Edge                        tabs, then caption 326 to 806; buttons from 814
Explorer                    tabs, then caption 294 to 806 (TITLE_BAR_SCAFFOLDING_WINDOW_CLASS)
Word                        caption 294 to 582 left of the search box, and 774 to 806
VS Code                     caption 86 to 198 only (menu, command centre and layout buttons take the rest)
Calculator                  the app's own content to 758, caption 774 to 806 only
```

Found and fixed on the way:

- **Classic title bars answer "caption" over most of the minimise button** (job 0062: the tag
  covered it): Windows 11 draws its buttons wider than a classic window's own hit test assumes.
  The buttons are now taken from Windows (`DWMWA_CAPTION_BUTTON_BOUNDS`).
- **A tab's chip cut its title short** ("Probe Termi…", job 0063): a text box exactly as wide as
  the measured text can still get an ellipsis; boxes now have a pixel to spare.
- **A reading taken while a window moved was kept** (job 0066: VS Code, pulled out of a group,
  stayed below a band, read at its tab's size): a reading taken at a size the window no longer
  has is taken again at once.
- **A guess moved a window twice** (job 0067: Calculator, made the visible tab of a wider group,
  left its band on a stretch guessed from its narrower size, 224 px instead of 32, and went back
  24 ms later): only a reading at a window's own size moves it into or out of a band
  (`titlebar::needs_band`, with a test).
- **A tag above a window hung over the zone above it** (job 0067: Calculator is at least 502 px
  high; in a 508 px zone, below the 28 px band, it keeps that height and runs 22 px into the
  band, and its tag, drawn above the window, covered the bottom of the next zone): tags above
  windows stay in their zone's band (job 0068: 3 px above the window's top edge, inside the zone).
- **The probe pressed a chip while its strip was still moving** (job 0066, Git Bash): it now
  measures and presses only once no title bar is being read (`title_bars_reading` in
  `mosaikctl state --json`) and two reads 300 ms apart agree.
- **An app shown again answers differently for a moment** (job 0070: Calculator, shown again as
  a tab, had room for a strip in one reading and none 40 ms later, and moved out of its band
  and back): a window moves into or out of a band only when a second reading, at least 250 ms
  later, agrees (`titlebar::band_step`, with a test). Job 0071: one move.
- **A reading right after a resize can catch the app mid-layout** (job 0071: VS Code, read 3 ms
  after it was made narrower, had one free stretch instead of two and stayed below a band): a
  window read at a new size is read once more 300 ms later. Job 0072: VS Code pulled out of a
  group wears its tag in its title bar again. Readings take 3 to 30 ms each.

Where apps' tags went (`--apps`, jobs 0062, 0063 and 0067, each app once per run): Notepad,
Notepad++, Paint, Edge, Chrome, Windows Terminal, Explorer, Word, Excel and Git Bash in their
title bars, 0 px from the zone at every step (into the zone, made a tab, hidden and shown by its
chip, pulled out), and their strips there too. VS Code's tag fits its one free stretch, left of
the command centre; its strip does not, so as a group's visible tab it goes below a band, with
the strip above it, and pulled out again it wears its tag in its title bar. Calculator keeps only
40 px of caption, so it wears its tag above it, below a band; in the probe's half-height zone it
keeps its minimum height (856 x 502 where 856 x 480 was given), 0 px from where that size fits
best. The window that moves itself with no caption to grab (`--self-move --no-title-room`) wears
its tag above it too, 0 px from its place.

### Two monitors (`mosaik-probe workspace --monitors`, job 0072)

The main monitor (1920x1080 at 0,0) and a virtual display (800x600 at 1920,0), both at 100%. The
probe's editor on the left half of the main monitor, its terminal and review windows side by
side on the virtual display, with the real mouse:

```
saved across both monitors:   3 windows, 3 zones, 2 monitors in the layout; each window 0 px, tag in its title bar
piled on the main monitor, then the workspace opened again (29 ms): each back on its own monitor, 0 px
review dragged by its title bar into the editor's zone on the main monitor: 0 px; and back: 0 px
its ring and tag on the virtual display, read from the screen: the zone's colour on all four sides and the tag
terminal dropped on the editor's tag, across monitors: a group of two on the main monitor, 0 px
its tab pulled out onto its own zone on the virtual display: 0 px, the editor shown again
```

Two monitors at different scales are still to be measured: the virtual display's scale is a
Windows setting (the release checks).

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

### Budgets, measured on the release build

- **At rest** (job 0056; `mosaik.exe` built by `mosaik.ps1 build`, started with its data in a
  scratch folder, no window open, 20 s to settle, then 60 s):

  ```
  mosaik.exe        CPU 0.026% of one core (0.0009% of the machine), private 4.1 MB, working set 20.4 MB, 243 handles
  mosaik-watchdog   CPU 0.000%,                                      private 1.0 MB, working set  6.6 MB,  87 handles
  ```

  Budget: under 1% CPU, and under 15 MB private for the two together with no web window open.
  Again after tags moved into title bars (job 0074): `mosaik.exe` 0.000% of one core, private
  4.0 MB, working set 20.8 MB, 241 handles, 6 threads; the watchdog 0.000%, 1.0 MB. The engine's
  share is now a test, `mosaik-engine/tests/budget.rs`, run by `mosaik.ps1 test` and CI: the
  engine started as the app starts it, with nothing to manage, measured for 10 s (job 0074:
  passed).
- **Switching eight windows** (job 0057, `mosaik-probe workspace --eight`): eight windows
  between a 4 by 2 grid and the same grid mirrored, so every window moves on every switch, 20
  switches: mean 80.7 ms, median 79.2 ms, slowest 107.5 ms, none failed (budget: 400 ms). The
  engine's own log agrees (`workspace opened ms=60.2` to `103.8`, `moved=8`).
- **Zone preview**: in the next frame after the window moves (step 1).
- **The owner's steps on Windows** (job 0056, Windows PowerShell 5.1): `mosaik.ps1 build` in
  173 s, `mosaik.ps1 test` in 108 s, every test passing (Rust tests, clippy, the interface's type
  check, lint and 39 component tests).

## Step 8: release

- **Store package** (`installer/msix/package.ps1`; `mosaik.ps1 package`): makepri indexes the
  logos at every size, makeappx packs (job 0038): `Mosaik_0.1.0.0_x64.msix`, 2.6 MB, 30 files:
  `mosaik.exe` 4.3 MB, `mosaik-watchdog.exe` 0.35 MB, `mosaikctl.exe` 0.59 MB,
  `THIRD-PARTY-NOTICES.txt`, `LICENSE.txt` (CC0 1.0 legal code), 21 logos. The first attempt
  failed makeappx's manifest validation (logos named in one size only), fixed. Not yet installed
  from the package: that needs the Store, or Developer Mode on this PC (off).
- **Release candidate** (job 0075, the owner's steps with Windows PowerShell 5.1 at commit
  2979949): `mosaik.ps1 test` in 54 s, every test passing (Rust: core 140, IPC 20, Win32 12
  with the real-window tests, overlays 2, engine 2 with the budget test; clippy clean; the
  interface's type check, lint, format and 46 component tests); `build` in 115 s; `package`:
  `Mosaik_1.0.0.0_x64.msix`, 2.6 MB, 28 files; `pack`: the one-file `mosaik.ps1`, 0.9 MB, 223
  files, checked file by file against the repository.
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

## 1.0.1: first builds

Called 1.1 then (the owner keeps the version at 1.0, see the owner's decisions). What the owner
asked for on 2026-10-12, and the evidence. Jobs run on the owner's PC (1920x1080
at 100%, two virtual monitors attached), with `mosaik-probe smart` (the engine in the probe's
process, its own windows, a scratch folder).

- **Zones that follow a resize** (ADR 0013, job 0092): dragging the line between Code and
  Terminal 150 px left, a line 100 px down and back up 50 px: the zones on both sides and their
  windows followed, every window 0 px from its zone; with Shift, the window alone (+60 px), the
  zones kept. A line pushed 400 px into a window that will not get narrower than 710 px stopped
  at 296 px: the zones made room for the app. The windows were then left 104 px from their zones:
  the check after a move compared a window moved twice with its first place
  (`workspace::last_places`, with a test that failed before the fix).
- **Arrange** (ADR 0014, jobs 0092 and 0093): four windows and one that cannot be resized, no
  workspace open: a new workspace "Workspace 1" in quarters in 44 to 46 ms, every resizable
  window 0 px from its zone, the fixed one centred across its zone and at its top (0 px), the top
  row's tops on one line. Undo: every window back, 0 px, in 16 to 17 ms, and no workspace left.
  Arranging twice moved nothing; a fifth window became a tab (a strip of two), the others did not
  move, and undo brought it back, shown.
- **Zones 4 px apart** (job 0093): `workspace --drag` and every kind of app (`kinds`), all 0 px
  from their zones; settings of format 2 moved to the new space once, with the note shown in the
  app's window.
- **The quick menu** (ADR 0015): the handle at the right edge opened its menu when the pointer
  rested on it, on the owner's desktop (2026-10-12, seen on screen: "No workspace open", Arrange
  windows, Switch workspace Win+Alt+Space...). In the probe the pointer reached the handle and
  the menu did not open (job 0093); Mosaïk now opens it from its own hover timer, and the probe
  says why when it does not (the windows under the pointer, the engine thread's input state).
- **No blank space above windows** (the owner: "we shouldn't have blank lost space"): a
  development build put every window below a 28 px bar at the top of its zone; tags are back in
  title bars by default, a window alone is never lowered (a tag with little room shows the
  zone's initial, or the window wears its ring alone), and settings of formats 2 and 3 move back
  once (format 4). Checked by tests: core 184 (`tag_cut`, the initial's room in a title bar, both
  migrations, failing before the change), the interface's 50 and 13 smoke tests in a browser;
  clippy clean for Windows. The site's desktops show it (screenshots at desktop and phone width).
- **On Windows without the mouse** (jobs 0094 and 0095, commits fbcc568 and 5f6bcce; the owner
  was using the PC): every test passing, core 184, IPC 20, Win32 13 (real windows), overlays 5,
  engine 2 (debug, once the library Smart App Control had refused was made again, and release);
  clippy and rustfmt clean; the interface's type check, lint, format and 50 tests. The release
  build (161 s) and the Store package (`Mosaik_1.1.0.0_x64.msix`, 2.72 MB). `mosaik.ps1 test`,
  the owner's own path, passed in 69 s ("Every test passed."), and it found in the real Code
  Integrity log the two files of this build Windows refused that day. `pack`: the one-file
  `mosaik.ps1`, 236 files, checked file by file against the repository; it and `mosaik1.sh`
  are beside the repository. Mosaïk started on the new build (16:45; the owner had quit the
  one before at 16:40, a clean exit in its log, no error reported to Windows), with the switcher
  shortcut registered, the watchdog running and the note about tags back in title bars shown.
- **A note said at every start** (found in job 0094): settings moved forward on load were not
  written back, so the move, and its note, came again at each start until the user changed a
  setting. A file moved forward or mended is now written back at once (a test failed before:
  core 185).
- **A home page that shows before it moves** (ADR 0016; the owner: "usable by an absolute
  noob", "anim and preset on homepage"). Locally: core 190 (a layout replaces a monitor's zones,
  fewer zones than windows make tabs, other monitors left to Arrange, layout ids and names the
  same as the interface's), IPC 10 (protocol 4 on the wire), the interface's 59 tests (the home
  page: the screen now, a layout previewed by pointer and by keyboard, used, undone, errors,
  nothing open) and 15 smoke tests in a browser, axe included; clippy clean for Windows. On the
  owner's PC with the real mouse (job 0099, commit 0a978c7):
  - `smart`, a layout (Thirds) over four windows in quarters: the preview answered in 2.7 ms
    with the three thirds as zones and changed nothing; Arrange with it took 49.8 ms ("Arranged
    4 windows in the “Thirds” layout; 1 window as a tab."), and each window landed 0 px from
    where the preview said, the fourth a tab; Undo: every window back, 0 px, the quarters back.
  - The picture of a screen Arrange has just arranged shows nothing moving (0 px), the window of
    a fixed size included (the preview used to show it filling its zone).
  - Arrange: a new workspace in quarters in 47.7 ms, every window 0 px from its zone, the fixed
    one centred at the top (0 px), Undo in 19.5 ms (0 px), twice moved nothing; a fifth window
    became a tab and came back with Undo.
  - The quick menu: opened 320 ms after the pointer rested on its handle, closed 215 ms after it
    left, dragged 120 px and kept there, hidden while a game filled the screen and back 65 ms
    after it left; its rows end with "Open Mosaïk".
  - `workspace --drag`: every window 0 px from its zone after saving and switching, rings in the
    zones' colours, tags in title bars, tabs made, clicked and pulled out by the mouse.
  - `kinds`: a borderless app 0 px from its zone, as a tab and pulled out again; games of a
    fixed size kept their size, where `fit_size` puts them (0 px); a game in full screen was
    left alone.
  - On the owner's own screen (six windows), asked without moving anything: what Arrange would
    do, 11 ms (four zones, two windows as tabs), and the Thirds layout, 10 ms; no workspace
    opened.
  - `mosaik.ps1 test` passed in 66 s; the release build (152 s, one file refused by Smart App
    Control made again) and the Store package (2.74 MB); `pack`, 240 files, checked against the
    repository; `mosaik.ps1` (1.09 MB) and `mosaik1.sh` (site 1.3.0) beside it.
  - What the run found, in the probes: the line between zones was looked for half way along an
    edge, where two neighbours meet and Windows answers with a corner; the fifth window's
    message changed ("1 window as a tab"); the band drag ran for a window with no band; the probe
    could not give its window the keyboard from the background (the quick menu's keyboard check
    did not run); Mosaïk's window opened behind the app in front, so the tour could not click it.
    Each is fixed for the next run (commit 6c55fc6). Mosaïk was not running when the job
    started: its log shows a clean exit at 17:01 ("windows restored before exit"), Mosaïk quit
    from its menu, not a crash.
- **Again with the mouse after the fixes** (job 0100, commit f6a15f6), every check passing:
  `smart`'s one line (tops on one line, 0 px), the line between zones moved 150 px left, 100 px
  down and 50 px back up from below, pushed back by an app that keeps 710 px (300 of the 400 px
  asked), an edge on the screen's border, and Shift (the window alone, 60 px); Arrange, its
  picture and the Thirds layout as in job 0099 (preview 2.1 ms, Arrange 49.8 ms, 0 px); the
  quick menu with the window in front given the keyboard first, and keeping it while the menu
  opened and Arrange ran from it; `workspace --drag` and `kinds` with nothing failing.
  `mosaik.ps1 test` passed in 88 s (the three new tests of starting with Windows among them),
  the release build in 100 s, the package. The tour of Mosaïk's window (72 s): 31 pictures of
  every page in light and dark, at the usual and the smallest size, the switcher, and the home
  page with Thirds and Arrange pointed at, on the owner's real windows (the preview named
  "Steam Client WebHelper" and "Windows Terminal Host": apps now have the names people know,
  `known_app_name`, core 191). Scrolled down to the lower layouts, the picture was out of view:
  it now stays at the top of the page, and the keyboard never lands under it (two smoke tests
  that failed first, 17 in all). Opened by hand, Mosaïk's window could not be told from .NET's
  "main window" (the quick menu's handle, a window with no title, came first); job 0101 looks
  for it by title. A second copy started with `--background` left at once (exit 0).
- **The last build** (job 0101, commit 1d7908b): `mosaik.ps1 test` passed in 53 s (core 191,
  Win32 9 and 7, IPC 10 and 10, overlays 5, engine, the interface's 59); the release build
  (79 s) and the Store package (2.74 MB). Opened by hand, Mosaïk's window showed 0.3 s after it
  started; started with `--background`, no window, and a second such copy left at once (exit 0)
  without showing the first one's. The tour: 31 pictures, the home page's picture in view with
  Thirds pointed at. On the owner's screen, asked only (no workspace opened, nothing moved):
  Arrange in 10 ms, zones Steam, Terminal, Settings, Browser for Steam, Windows Terminal,
  Settings and Google Chrome, Claude and File Explorer as tabs; Thirds in 10 ms, zones Browser,
  Files, Chat. `pack`: 240 files, checked against the repository; `mosaik.ps1` (1.09 MB) and
  `mosaik1.sh` (site 1.3.0) beside it. Mosaïk left running in the tray.
- **Arrange minimises, windows move only when asked, a workspace's apps** (ADR 0017 and 0018; the
  owner: "arrange window (auto) is really fucked up", "it should never resize when opening a
  window", "the position of tabs", "see and set list of current apps in a workspace"). Job 0104
  read what Arrange had done on the owner's screen: eleven windows into a workspace of three
  zones, eight as tabs of one group; and a window brought forward had its band changed by a
  title bar read again. Locally: core 202 (grids by monitor size, leftovers minimised, the
  windows used last keep the zones, app edits, names for older apps), IPC 10 (protocol 5), the
  interface's 62 tests and 18 smoke tests (the apps panel included); clippy clean for Windows.
  On the PC with the real mouse (job 0105, commit 6ae308f): `mosaik.ps1 test` passed in 62 s
  (core 201, Win32 9 and 7, IPC 10 and 10, overlays 5, engine, interface 62).
  - `smart`, Arrange: a new workspace in quarters, 0 px; Undo 0 px; twice moved nothing; a fifth
    window minimised ("Arranged 4 windows in 4 zones; 1 window minimized.") and back with Undo,
    no tab; Thirds over four windows: the preview (3.1 ms) said where each goes and that one is
    minimised, Arrange (42.9 ms) put each 0 px from it, Undo brought zones and windows back.
  - Brought to the front one after the other, twice (8 times), as Alt+Tab does: the windows
    moved 0 px.
  - The apps of the open workspace: 4 listed in their zones, a new window offered (not the
    workspace's own), added to a zone in 28 ms and 0 px from it, moved to another zone (0 px),
    taken out (it stayed, 0 px); the workspace's windows moved 0 px meanwhile.
  - The quick menu: open 70 ms after the pointer stopped on its line, rows "Workspace 1",
    "Arrange windows", "Switch workspace", "Save layout", "Edit zones", "Snap while dragging",
    the window in front keeping the keyboard; closed 269 ms after the pointer left. Two
    failures, fixed for the next run: picked from the menu, Arrange ran but the menu came back
    (its slide away moved it under the still pointer, which Windows reports as a move: such
    moves are ignored now); and the handle could not be dragged, the menu now opening before a
    press (the menu is dragged by its first line now).
  - `workspace --drag`: tab strips at the same place from the frame's right and top, 157 and
    9 px, whichever tab shows (pictures of both); zones, tags and tabs with the mouse as before.
    `kinds` as before.
  - The tour: 33 pictures, the Workspaces page's apps and the windows it offers among them; the
    welcome, home, settings and about pages with less text. Opened by hand, Mosaïk showed its
    window in 0.2 s; with `--background`, none. On the owner's screen, asked only: Arrange
    would make six zones for ten windows and minimise four, among them Claude, the window in
    front; the windows used last now keep the zones (a test that failed first).
  - Release build (79 s), Store package (2.76 MB), `pack` (244 files, verified), `mosaik.ps1`
    and `mosaik1.sh` beside the repository dated that day, the site folder made again (1.4.0).
  - Again after the fixes (job 0106, commit 22cd2b2), every `smart` check passing: the quick
    menu open 55 ms after the pointer stopped, closed after Arrange was picked from it (the
    windows 0 px from their zones, the window in front keeping the keyboard), closed 351 ms
    after the pointer left, dragged 120 px down by its first line (the menu followed, 120 px,
    and stayed open; the handle then 120 px lower, the place kept: 416 thousandths); out of the
    way of a game in full screen and back 72 ms after it left. Arrange, Thirds, the fifth
    window, windows brought forward and the apps as in job 0105 (0 px throughout). On the
    owner's screen, asked only: Arrange would keep Claude, Google Chrome, Steam, Notepad++ and
    both Calculators in six zones and minimise the four used longest ago (Battle.net, MSYS2,
    Windows Terminal, File Explorer); Thirds would keep Claude, Google Chrome and Steam. The
    tour (33 pictures), opened by hand (window in 0.2 s), `--background` (none), the release
    build (78 s), the package (2.77 MB), `pack` (244 files, verified), both files beside the
    repository; Mosaïk left running in the tray.

- **Asked 2026-10-12, 23:10 to 23:42** (ADRs 0019 and 0020): Arrange should "assess the main
  window (VS Code is superior to a terminal) and make a split view depending on the task";
  apps need proper names; the home page should list each workspace's apps; on the site,
  "Free and open source" and "Requirements" beside the button, on two lines, and a less
  stacked hero; no coloured borders and no space between windows; Battle.net "too tall" in
  Quarters. Job 0108 (commit 87f0937):
  - `mosaik.ps1 test`: every test passed in 59 s (core 207, among them the task layouts on six
    screen shapes with minimum sizes, the settings moved to format 5, windows edge to edge;
    the interface's 64).
  - `smart`, every check passing: with no workspace open, four probe windows of no known kind
    were laid out for the task "general": the window used last in the main zone (1056 x 1032,
    55%), three beside it of 344 px each, the window of a fixed size kept at its size, at the
    top of its zone, centred (0 px); Undo 0 px; arranged twice, the second moved nothing (0 px,
    and the picture showed nothing moving). A fifth window, opened last, took the main zone,
    the one used longest ago was minimised, Undo brought every window back (0 px). Thirds, the
    windows brought forward (8 times, 0 px moved), the apps of a workspace (added, moved,
    removed, 0 px) and the quick menu (open 51 ms after the pointer stopped, closed after a
    pick, dragged 120 px, out of the way of a game in full screen) as in job 0106. New, 3h: a
    window told to go no smaller than 640 x 619 was read from another process as 626 x 612,
    its frame without its invisible borders (0 px off); Quarters (960 x 516) minimised it, in
    the picture and on screen, and put the four others in the quarters; Undo 0 px.
  - `workspace --drag`: three windows saved and switched back 0 px from their zones, edge to
    edge, without rings (settings format 5: rings off, no space), each zone's tag in its title
    bar in its colour; tabs, drag to snap and pictures of the strips as before. `kinds` passing.
  - On the owner's screen, asked only (Claude, Battle.net, MSYS2, File Explorer, Google
    Chrome): Arrange read no task in particular; Google Chrome took the main zone (1114 x 1032),
    Claude (568 px high) and MSYS2 (464 px) beside it. Three windows of 344 px beside it did
    not all fit their apps' minimum sizes, so two were kept and File Explorer was minimised;
    Battle.net, wider than any zone beside the main one (768 px at most), was minimised too.
    Quarters and Thirds would minimise Battle.net as well, too big for their zones.
  - Release build (135 s), Store package (2.79 MB), the tour (33 pictures, the home page with
    each workspace's apps), opened by hand (window in 0.3 s), `--background` (no window, the
    settings moved to format 5), `pack` (247 files, verified), `mosaik.ps1` and `mosaik1.sh`
    beside the repository dated that day, the site folder made again (1.5.0); Mosaïk left
    running in the tray.

- **Asked 2026-10-13, 00:42 to 00:51** (ADRs 0021 and 0022): "instead of letting it overlap or
  minimising the app, reorganise the size of the zones", "not becoming the main zone, just
  taking the minimal space"; on the site, the header's Download button alone; sets of the apps
  on the PC that go together ("terminal, Chrome, Claude, VS Code and File Explorer"), secondary
  but visible early, and the windows open the first time sorted into workspaces; "clean,
  concise, non-invasive, easy to understand". Jobs 0110 to 0113 (commits 647f225 to 84f922b):
  - `mosaik.ps1 test`: every test passed in 57 to 60 s (core 222 in job 0113, among them zones
    that make room in every preset, the owner's sets, a set's workspace found by its apps and the
    data folder; the interface's 68).
  - `smart`, every check passing (job 0111): 3h, a window told to go no smaller than 1152 x 619
    (read from another process as 1138 x 612, its frame, 0 px off): in Quarters its quarter
    grew to 1138 x 612 and the others gave up the room; it was not minimised, the zones still
    filled the screen, and of five windows for four quarters one other was minimised
    ("Arranged 4 windows in the “Quarters” layout; 1 window minimized."); Undo 0 px. 3i, a window
    that will not get shorter than 567 px (560 px of frame) dragged with the mouse into the
    smallest zone (864 x 344): its zone grew to 864 x 560 and the two below gave up the room
    (236 px each); every window fit its zone (0 px), the one of a fixed size kept its size,
    centred across its zone and at its top (0 px). Job 0110 ran 3i with a check that compared
    the zone with the window's outer height and measured the fixed window as if it filled its
    zone: the probe was wrong, the zones the same. The rest as in job 0108.
  - On the owner's screen, asked only (Claude, Notepad++, two File Explorer windows): Arrange
    read coding; Notepad++ in the main zone (1056 x 1032), and beside it Claude's zone made
    392 px tall for it, the two File Explorer zones 320 px each, rather than three of 344 (12 to
    17 ms). Thirds put three in thirds and minimised one; Quarters fit all four. With Windows
    Terminal and Steam open too (job 0113): Notepad++ 1056 x 702 with the Terminal under it
    (1056 x 330), Claude 864 x 392, the File Explorer windows 864 x 320, Steam minimised.
  - `mosaik-probe sets`: Mosaïk reads the same Start menu as Windows' own `Get-StartApps` (139
    apps, 35 packaged, 104 with their program; 163 to 172 ms). Job 0110's choices took "Windows
    Software Development Kit" (explorer.exe opening a folder) for File Explorer, offered both
    Teams and Calendar as chat and mail, and Notepad++ open would have taken VS Code's place;
    after the fixes, tests first (jobs 0111 and 0112): Coding, "Visual Studio Code, Terminal,
    Google Chrome, Claude, File Explorer"; Design, "Canva, Google Chrome, File Explorer, Claude";
    Writing, "Word, OneNote, Google Chrome, Claude". File Explorer started through its Start menu
    entry by Mosaïk's own code showed a new window in 5.4 s in job 0110 and none in jobs 0111
    and 0112 (30 s). Character Map, started through its entry on a thread like the engine's,
    with COM as `ShellExecuteEx` asks (job 0113): started in 216 ms, its window in 490 ms,
    closed.
  - The windows open sorted into workspaces as at the end of the welcome, in the scratch data
    (job 0110): "Your open windows make 1 workspace: Coding." (Notepad++, Claude and the two File
    Explorer windows, each in a zone named for it); 0 of 30 windows moved.
  - A program started with `APPDATA` and `LOCALAPPDATA` changed, as the jobs started Mosaïk on
    the scratch data, sees only the Start menu's packaged apps: 35 of 139, for `Get-StartApps`
    too (job 0111), so that Mosaïk offered "Chat and mail" alone. Since job 0112 it gets its
    folder from `MOSAIK_DATA`: its log says "Start menu read apps=139 packaged=35
    with_program=104", and it offers the three sets above. The tour then stopped at Thirds: the
    sets, as cards, took two rows and pushed the layouts out of view in Mosaïk's window
    (980 x 720); they are now one line under Arrange, "Open the apps for" and a button per set,
    and the tour passes (job 0113, 33 pictures). None of the three sets has a workspace yet: the
    welcome's "Coding" (Notepad++, Claude, File Explorer) is not the Coding set's, which is found
    by its apps; opening a set, which starts apps on the owner's screen, has not been run there.
  - `workspace --drag` (job 0113): saved and opened again, 3 of 3 windows in their zones, as
    before the apps' launch thread changed.
  - Release build (81 to 136 s), Store package (2.84 MB), opened by hand (window in 0.3 to
    1.0 s), `--background` (no window; protocol 6, watchdog, Win+Alt+Space registered), `pack`
    (255 files, verified), both files beside the repository dated that day, the site folder made
    again (1.6.0, job 0110); Mosaïk left running in the tray on the scratch data.

- **Asked 2026-10-13, 03:02 to 03:11** (ADR 0023): "the app is way too complicated: simplify
  without losing value and functions"; "it shouldn't open an app twice: if it's open, take the
  one opened"; "when opening the code preset it doesn't organise right away"; "we should really
  rarely use more than 3 windows in a view"; "for writing and design, why would someone need
  Claude"; "the tabs go behind the new window" when another tab of a zone is clicked; "opening
  the options of the last on the list doesn't show them"; the quick menu's line "has to be pixel
  perfect" and should "never open while playing an FPS"; on the site, a line beside the title,
  and the first picture's windows "kind of overlap"; "what is the Check workspace"; and the
  installer (the owner kept the Store's). Jobs 0116 to 0119 (commits 63b7e9b to 617e0a4):
  - `mosaik.ps1 test`: every test passed in about 60 s (core 231, among them three windows to a
    view, a set's other apps as tabs with the app they go with, the writing and design sets
    without an assistant, apps the same by id without case or by program, where the quick menu's
    line goes; Win32 10, among them the folder an app starts in; the interface's 73, among them
    a menu that opens upward near the bottom of the window).
  - First against the engine without the fixes (side commits, jobs 0116 to 0118): a closed probe
    window that shows cloaked first, as Chrome, VS Code, Claude and Windows Terminal show
    theirs, with its workspace opened twice before it showed, started twice (2 copies) and never
    took its zone (1087 px off); a game window over the work area hiding the pointer: the quick
    menu opened when the pointer went onto the line. With the fixes: 1 copy, 0 px (jobs 0117 and
    0118); the menu stayed closed over the game and opened once the game was closed (job 0118).
    Keeping the pointer shapeless over the line did not work (job 0117): over the line the
    pointer is Mosaïk's own (Windows keeps one per program); the pointer is now looked at five
    times a second while a window covering the main monitor is in front.
  - The sets saw Windows Terminal as not open (its windows carry no AppUserModelID of their own,
    job 0113): now "Coding: Visual Studio Code, Terminal (open), Google Chrome (open), Claude
    (open), File Explorer"; "Design: Canva, Google Chrome (open), File Explorer"; "Writing: Word,
    OneNote, Google Chrome (open)" (job 0116).
  - The tab strip above the window shown after each of three tab clicks (job 0116, `workspace
    --drag`).
  - `smart`, every check passing (job 0118): three probe windows of no known kind (two that can
    be resized, one of a fixed size) laid out in a new workspace in 8 ms: the window used last in
    the main zone (1114 x 1032), two beside it, the fixed one at its size at the top of its zone
    (0 px); Undo 0 px; arranged twice, nothing moved. A fourth window, opened last, took the main
    zone and the one used longest ago was minimised ("Arranged 3 windows in 3 zones; 1 window
    minimized."); Halves put two in halves and minimised one; 3h, a window that will not go below
    1152 x 619 (read as 1138 x 612, its frame) got a quarter grown to 1138 x 612 in Quarters and
    no window was minimised; 3i, a window dropped in a zone too short for it got a zone grown to
    560 px; the windows brought forward 6 times, 0 px; a workspace's apps edited, 0 px. The quick
    menu's line on the left edge (0,271 to 8,347), since the owner's two virtual screens are on the
    right and the pointer does not stop at that edge; it opened 55 ms after the pointer rested on
    it, then as before (Arrange from the menu, closing, dragging, out of the way of a game in full
    screen), and stayed closed over the game hiding the pointer (4g).
  - The line's move to the other edge was said in a notice that opened Mosaïk's window at every
    start (job 0116); it is now a note, once ever, shown when the window is next opened.
  - Two screens at two scales (job 0116, `workspace --monitors`): the main one at 100% (96 dpi)
    and a virtual 2560 x 1440 one at 125% (120 dpi). Saved and opened again, every window 0 px
    from its zone on both; a window dragged onto a tag on the other screen joined it as a tab,
    pulled out it went back to its zone there (0 px), its tag in its zone's colour.
  - Every kind of app (`kinds`, job 0116) passing; Character Map started through its Start menu
    entry, its window in 450 ms, closed.
  - Steam, started by the Mosaïk in the tray, wrote its log `logs\webhelper.txt` in the folder
    Mosaïk was started from (the repository, job 0115): apps now start in their own folder, or
    the user's for a Start menu entry (test first). The file was left for the owner to delete.
  - "Check" was a workspace saved from the switcher in the test data (`%TEMP%\mosaik-first-run`)
    on 2026-10-12 at 02:17, beside "Workspace 1" to "Workspace 3", "Coding", "Writing", "Design"
    and "Coding 2". Since job 0116 the Mosaïk in the tray runs on the owner's own data, empty at
    first: the welcome shows once.
  - The tour (jobs 0117 and 0118, 29 pictures, every control named): Home with a workspace's ⋯
    menu, which opened upward near the bottom of the window at the usual and smallest sizes, its
    apps and the windows it could take, the zone editor opened from it, Settings with More
    settings and About, the switcher; in light and dark and at the smallest size. Job 0116's tour
    stopped at Thirds: the note about the quick menu's edge, at the top of the page, pushed the
    layouts down out of view; the tour now brings what it points at into view, as the keyboard
    does.
  - Release build (77 to 82 s), Store package (2.85 MB), opened by hand (window in 0.3 s), on the
    owner's own data with `--background` (the welcome, once: Mosaïk had not been set up there),
    `pack` (258 files, verified), both files beside the repository dated that day, the site
    folder made again (1.7.0, job 0116); Mosaïk left running in the tray on the owner's own data.
  - A Character Map window stayed open on the owner's screen after job 0116 (started 04:34:56 by
    its sets probe, which had closed another window of the program): closed in job 0119, as its
    close button does. The probe now closes the window the engine would manage; run again (job
    0119): Character Map's window in 465 ms, 1 window, closed, none left open.

## 1.0.1: the window rebuilt

Called 2.0 until 2026-10-13 (the owner keeps the version at 1.0, see the owner's decisions).
What the owner asked for on 2026-10-13, after opening 1.0 from the Store ("break everything and
redo everything", "the dark theme default was much better", "production ready"), and the
evidence (ADR 0024). Jobs 0122 to 0133 ran on the owner's PC: the main screen 1920 x 1080 at 100%,
two virtual screens of 2560 x 1440 at 125% to its right (so the side menu's line is on the left
edge, where the pointer stops).

- **The window, rebuilt**: Home (the screen as the chosen layout would leave it, Smart and the
  ten layouts, Arrange and Undo; the workspaces beside it, then the suggested sets), a
  workspace's page, the zone editor, Settings. In a browser (2026-10-13): type check, lint and
  format clean, 92 component tests, 28 smoke tests in Chromium (light, dark, high contrast, the
  keyboard path, Mosaïk not answering, a window dragged onto another). On Windows,
  `mosaik.ps1 test`, "Every test passed" (job 0133: core 246, IPC 12 and 10, Win32 11 and 7 with
  real windows, overlays 8, engine 2 and 1, the interface's 92, clippy with warnings as errors,
  and the scripts' own tests below); rustfmt clean. The tour (jobs 0122, 0123, 0126): 29 pictures,
  every control named, on Home, a workspace's page, the zones and Settings, dark, light, at the
  smallest size and with keyboard focus.
- **Dark by default** (settings format 6): the owner's settings, written as format 1 by the
  Store's 1.0 at 14:52 with the theme following Windows, read by 2.0 as dark ("format 6; theme
  dark; start with Windows False; side menu True right", jobs 0122, 0123, 0126).
- **Apps by their own icons**: a window by its program, a packaged app by its id. Job 0122 showed
  no picture for Chrome (its windows say "Chrome", which the Start menu knows by another id):
  desktop apps are now keyed by their program (test first). Job 0123 showed only the Terminal's
  among the suggested sets: a set starts a desktop app through its Start menu entry, which is now
  its key too, and File Explorer through its own entry (tests first:
  `every_app_of_a_set_shows_its_start_menu_picture`,
  `an_app_started_by_its_start_menu_entry_is_known_by_that_entry`). Job 0126,
  `mosaik-probe sets`: the Start menu read in 184 ms, the same 139 apps as `Get-StartApps`; 11 of
  11 apps of the three sets with their own icon, read as Mosaïk's window reads them (PNGs of 1.3
  to 4.1 KB: Visual Studio Code, Terminal, Chrome, Claude, File Explorer, Canva, Word, OneNote),
  and on Home in the tour's pictures.
- **No alerts**: a toast by the corner of the pointer's screen, pictured in both themes (job
  0126, `mosaik-probe menu-shot`, 384 x 108 px: "FancyZones is running. It moves windows too.
  Turn it off in PowerToys, or it and Mosaïk will keep moving the same windows."). Mosaïk's window
  opens only when asked: by hand in 0.2 to 0.3 s; started with `--background`, none (jobs 0122,
  0123, 0126).
- **The side menu** (job 0126, both themes): Mosaïk on its first line ("Open window"), Arrange,
  the five layouts, the workspaces with the open one marked, Save layout, Edit zones, Snap while
  dragging; the switcher's shortcut in full beside "Workspaces" (in job 0123 it was cut short,
  "Win+Alt+Spa…": measured in one weight, drawn in another). It opened 50 to 57 ms after the
  pointer rested on its line.
- **The probes** (job 0128, the engine in the probe's process, its own windows; every check
  passing): zones following a resize, 0 px; Arrange of three windows, one of a fixed size, into
  a new workspace laid out for the task in 14 ms (the main window in the largest zone, the
  fixed one at its size, centred at the top of its zone, 0 px), Undo in 18 ms (0 px, no
  workspace left), arranged twice with nothing moved; a fifth window opened last took the main
  zone and the one used longest ago was minimised; Halves in 15 ms (its preview in 3 ms, which
  moved nothing), Quarters making room for a window that will not go below 1152 x 619; a window
  dropped in a zone too short for it grew its zone; windows brought forward 6 times, 0 px; a
  workspace's apps edited, 0 px. The side menu: Arrange from it 0 px with the window in front
  keeping the keyboard; Halves from its strip (the window in front and the one used next in the
  halves, the third minimised, as Arrange decides: jobs 0122 to 0127 expected other windows,
  the probe was wrong, not the engine); closing 323 ms after the pointer left; dragged 120 px
  along its edge; hidden while a game filled the screen and back 76 ms after; never opened while
  the game hid the pointer. `workspace` (job 0126): saved in 24 ms, opened in 25 ms, 3 of 3
  windows in their zones, a closed app started again once, 0 px; the probe's memory with
  frames 8.5 MB private.
- **Windows dragged onto each other** (the owner, 2026-10-13: "in this view we should be able to
  drag drop apps to permute them"): on Home's picture a window dropped on another trades places
  with it, each keeping its zone's name and colour; one dropped on an empty zone of a layout
  moves there; Enter, Enter and Escape do the same from the keyboard; Arrange puts them so, Reset
  puts the picture back (protocol 8: `moves` on `arrange` and `preview_arrange`). Tests first in
  the core (traded with names and colours, into an empty zone, a window that needs more room
  making room again, moves that name nothing) and in the interface (four component tests, a
  smoke test that drags with the mouse in Chromium). Job 0132's probe found two windows of one
  app keeping their places' names ("Browser 2" went to the window that was "Browser"), while the
  colours followed: names are now told apart in the order of the zones before the move (test
  first, `windows_of_one_app_keep_their_own_zone_names_when_moved`). Job 0133,
  `mosaik-probe smart`, Halves with the first window dropped on the second: the preview in 1.4 ms
  traded them, with their names and colours, and moved nothing on screen; Arrange put them there
  in 12 ms (0 px each), Undo put them back (0 px). The tour (job 0133) dragged Chrome onto Claude
  in the picture with the real mouse: they traded places, Reset put them back, and no window
  moved (Arrange is never clicked).
- **Sending to the Store** (ADRs 0025 and 0026): `./mosaik1.sh --deploy --app` runs
  `mosaik.ps1 publish`, which sends the package through the Store's API with nothing to answer,
  then deploys the site. The Store's key is in `.env.local` at the top of the repository (git
  ignores it, `pack` leaves it out); without it, it is asked once before anything else and kept.
  The owner's first run (2026-10-13) reached the prompt after the tests and the build, and no
  paste got through Git Bash's window (Shift+Insert, then a right-click): nothing was sent. The
  key belongs to the Entra application "Mosaik Store publishing", which needs a Microsoft Entra
  tenant the developer account did not have: Microsoft blocked the first one the owner made
  ("Access to tenant is denied", AADSTS5000228, after its card check failed); the second,
  `gimz.onmicrosoft.com`, was made on 2026-10-13 and linked to the account that evening (Tenants:
  "Tenant association complete"). The application was made there with the Manager (Windows)
  role, then its first key (Partner Center shows it once; the owner keeps it). Its tenant and
  client IDs are in `identity.json`. `installer/msix/test-publish.ps1`, part of `mosaik.ps1
  test`, runs `publish.ps1` against a stand-in of Microsoft's token and submission services that
  checks every request (the token for the tenant, client and key given, the waiting submission
  deleted, a new one from the last published, the old package removed and the new one added,
  "What's new" in the listing and nothing else changed, the ZIP uploaded as a block blob, the
  commit, the status read until the Store has it): 16 checks passing with everything in the
  environment and nobody to answer; with the key in `.env.local` (a comment and quotes around
  it), nothing asked or kept; on the first run the key alone asked, with `identity.json`'s tenant
  and client, and kept encrypted; the next run asking nothing; without a key given, nothing kept
  or sent; `-KeyOnly` (run first by `mosaik.ps1 publish`) asking and keeping, sending nothing;
  `-Yes` without a key refused. The hidden prompt's own 4 cases: a key pasted with spaces and a
  line break around it, the key copied taken when nothing came through, other text copied not
  taken, nothing at all. publish.ps1 runs from a copy of the scripts, so the repository's own
  `.env.local` is never read: with one there, the test before failed ("checks not passing:
  token_form") and passes now. The `publish.ps1` before failed it (it
  asked for the tenant ID first) and the one now passes, here and on the owner's PC (Windows
  PowerShell 5.1, job 0143).
- **The build around a running Mosaïk.** The build step's own test
  (`installer/test-mosaik-ps1.ps1`): job 0132's `mosaik.ps1 build` stopped at once on Windows
  PowerShell 5.1 ("The property 'Count' cannot be found on this object"); job 0133 ran the test
  against the script as job 0132 had it (every case failing), as the next commit had it (the
  case where Windows does not start the new build failing) and as it was then (6 of 6 passing).
  Then `mosaik.ps1 build` with the owner's Mosaïk running from the folder: it quit, the build
  took 146 s, Mosaïk started again in the tray (a new process, the old one gone); with none
  running, 121 s. Smart App Control then refused the new `mosaikctl.exe` while `mosaik.exe` ran
  (job 0133), so the build could not ask Mosaïk to quit. Job 0134, with the owner's Mosaïk
  running and `mosaikctl.exe` still refused: the build stopped it, but its watchdog then showed
  "Mosaïk has stopped" and the build failed waiting for it. Now the watchdog is stopped first and
  its quiet rescue brings back the windows from the journal it watched (9 cases: the script
  before failed the new ones and the one now passes all 9, on the owner's PC, jobs 0134 and
  0135). `mosaikctl.exe` made again was allowed (job 0134). Job 0135: the build with Mosaïk
  running asked it to quit, built in 141 s and started it again in the tray (a new process:
  version 2.0.0, protocol 8, its watchdog, Win+Alt+Space registered), the same 14 windows on
  screen before and after. What `mosaikctl quit` answers is now shown, and a line of it on stderr
  (Mosaïk not answering, say) is no longer taken for Windows refusing to run it: Windows
  PowerShell 5.1 stops at such a line when the output is redirected. The test stands in for
  mosaikctl with Windows' own `tar.exe`, which refuses "quit" on stderr (`whoami.exe`'s answer
  reached the pipe as one empty line and `where.exe`'s as none, jobs 0140 and 0141): 10 cases,
  the script before (bbe6601) failing the new one ("its answer (Usage:) not shown") and the one
  now passing all 10, on the owner's PC (job 0141). The test also runs when Windows PowerShell
  5.1 starts it with -File and no -Script: its default path is set in its body (job 0139: started
  so, it stopped at once in Join-Path; job 0141: passing). Job 0139, with the owner's Mosaïk
  running: the build showed its answer ("Mosaïk is closing; every window it hid is coming
  back."), built in 130 s and started it again in the tray (a new process, `mosaikctl` 2.0.0),
  30 windows on screen before and after. `mosaik.ps1 test` now runs mosaik-probe's tests too
  (2), and passes whole on the owner's PC (job 0141, 53 s: 299 Rust tests, clippy, the
  interface's 92, the publish test's 16 checks, the build step's 10 cases, "Every test
  passed.").
- **Previews on the owner's own screen** (`mosaikctl preview`, asked only, job 0126): Smart in
  12 ms (Chrome 1114 x 1032 as the browser, Windows Terminal 806 x 568, Claude 806 x 464; File
  Explorer and two others minimised), Thirds and Quarters in 10 ms.
- **Another Mosaïk**: the Store's 1.0 held Mosaïk's pipe in job 0122. `mosaikctl` now says so
  ("it is another copy of Mosaïk (mosaik.exe), not this one", with how to quit it), and windows of
  Mosaïk's own programs are never arranged (the 1.0 watchdog's message had a zone in job 0122's
  preview; job 0123, none).
- **The Store's 1.0 opened while this build runs** (job 0126): from the Start menu, it handed
  over and left: this build's window showed 1.0 s later, and no 1.0 process was left 3 s later. 1.0 does
  not start with Windows on this PC (its startup task is off, state 0).
- **Built and packed** (job 0133): release build in 121 s, `Mosaik_2.0.0.0_x64.msix` 2.90 MB;
  `pack`, 270 files, checked file by file against the repository; `mosaik.ps1` and `mosaik1.sh`
  beside the repository, dated that day (each later copy is checked the same way when it is
  made; job 0139: the package 2.9 MB, `pack` 271 files). Opened by hand, Mosaïk's window shows in
  0.3 s; with `--background`, none (job 0133). This build left in the tray on the owner's own
  data (job 0135: version 2.0.0, as it was called then, protocol 8, its watchdog, Win+Alt+Space
  registered). As 1.0.1 (job 0143): `mosaik.ps1 test` passing whole in 81 s; the build with
  Mosaïk running in 143 s, started again in the tray (`mosaikctl 1.0.1`, `mosaik.exe` file version
  1.0.1); `Mosaik_1.0.1.0_x64.msix` 2.9 MB, an update of the Store's 1.0.0.0 installed on the PC;
  `pack`, 271 files, checked against the repository.
- **The site** (`mosaik1.sh` 1.10.0, with `--app`): its first home page back (the title, "Every
  project gets its own workspace: your apps and windows arranged and ready in one click.", "Free
  and open source.", desktops of two to four windows); the site folder is what the script writes
  (2026-10-13, again for 1.10.0); `shellcheck` clean; built and type-checked clean. A rerun writes
  its 25 files afresh and removes any it no longer makes, keeping `public/`, the Vercel link, the
  dependencies and the last build and type-check: 0.4 s, against 15 s for a first install here,
  and the type-check 2.4 s instead of 4.1 s. It empties only a folder it wrote (one with its
  `.mosaik-site`): a folder of notes, and one with a `Cargo.toml`, refused with nothing removed.
  `--app` checked with stand-ins for `powershell.exe` and `winpty`: the same command in Git
  Bash's own window and elsewhere, and a failed send stops everything with nothing deployed.

## 1.0.2

What the owner asked on 2026-10-14 at 00:22, with a screenshot: a workspace's page said "Open a
window to add it here." with about ten apps open ("i should be able to add claude for example");
"when drag drop a window as new tab of a zone it overlaps the tabs and we only see the window";
the site's section "One desktop, many kinds of work." gone, whole (ADR 0024).

- **Minimised windows offered**: opening a workspace minimises the windows it does not use, and
  its page offered only windows on screen. It now offers every open window not already the
  workspace's, minimised ones too; one added to a zone shows again in its place. Test first:
  smart's 3g minimises a window and expects it offered, then shown in its zone. Against the
  engine before (4b361eb, job 0146): not offered. With the first fix (af51cb7, job 0146):
  offered, but once added it stayed minimised, as the refit that places it leaves out every
  minimised window; `plan_refit` now takes the window just added (4c6a249; the core test
  `refit_shows_a_minimised_window_just_added` failed on the filter before, on every topology).
  With it (job 0147): offered, added, shown in its zone 0 px off, every check of smart's arrange
  part passing.
- **The strip above a dropped window**: the dropped window was raised after its strip went above
  it, and an app that comes forward late covered the strip. A dropped window, one pulled out of
  its group and the windows a workspace opens with now have their strips and frames put back
  above them twice in the next half second, as a clicked tab already had. Test first: the
  probe's dummy window can come forward 250 ms late, and `workspace --drag` drops it on a tag and
  checks the strip ends above it. Against the engine before (4b361eb, job 0146) the probe stopped
  at the next step: the terminal, dropped on the code window's tag, covered the strip in that
  window's title bar, so the code tab could not be clicked ("another window (MosaikProbeDummy, at
  0,0 to 1056,1032) is where the probe would press the mouse at 756,19"). With the fix (job
  0146): the strip above the late window after the drop, above the window shown after each of
  three tab clicks, after the drop at the tag's start, and after both drops onto the strip where
  Windows' snapping also watches; 3 of 3 windows in their zones; the joined terminal, the one
  pulled out and the one dragged back all 0 px from their places.
- **The Store while it checks the version sent before**: `publish.ps1` looks at the submission
  waiting in Partner Center before replacing it; one still being checked is left alone, nothing
  is sent, it says so and ends with 3, which `mosaik.ps1 publish` passes on, and `mosaik1.sh
  --deploy --app` warns and still puts the site up. Test first: the stand-in keeps a submission
  in certification and refuses to delete it, as Microsoft does; the publish test failed on the
  `publish.ps1` before (it never looked at the waiting submission) and passes now (17 checks, and
  the new case); the `mosaik.ps1` test's 3 publish cases failed on `mosaik.ps1` before (3 taken
  for a failure) and pass; `mosaik1.sh --app` with a stand-in publish ending with 0, 3 and 1:
  done; waiting, the site made after it; failed, nothing after it.
- **The site** (`mosaik1.sh` 1.11.1): no section "One desktop, many kinds of work.", no "Who
  it's for" in the header, the small desktops it drew gone; type-checked and `shellcheck` clean.
- Version 1.0.2: the changelog, and "What's new" with 1.0.1's for anyone still on 1.0.0.
- **On the owner's PC** (jobs 0146 and 0147, Windows PowerShell 5.1): `mosaik.ps1 test` passing
  whole (91 s, then 78 s), the publish test's new case and the 3 publish cases with it; the
  build in 133 s and 127 s, `mosaikctl 1.0.2`, `mosaik.exe` file version 1.0.2; `Mosaik_1.0.2.0_x64.msix`
  2.9 MB; `pack` 1.4 MB, 271 files, checked against the repository; `mosaik.ps1` beside the
  repository, and the owner's `mosaik1.sh` there brought to 1.11.1 with his key line kept at
  line 3, the rest the same as the repository's; Mosaïk 1.0.2 in the tray on his own data
  (protocol 8, its watchdog, Win+Alt+Space registered).

### Asked on 2026-10-15 (01:33 to 01:41), in 1.0.2

The owner, with two screenshots of a workspace's page and of "Coding" opened from Home: zone
names "like assistant code zone etc are absolute bullshit they should be 1 2 3 4 5 6"; the zone
list in each app row read as a list of apps, with "Zone" among them and "A tab" on some rows;
"show all dots and the selected one is highlighted", "the remove should be a cross", "for all
apps"; "show visually the zones same as homepage for that workspace specifically with
arrangements aswell"; on Home, "we shouldnt open the workspace when clicking on it it should show
it in the anim as preview and show an open button"; opening "Coding" left a game launcher and the
Riot client on top (his log: moved=3 minimized=0); and "the tab thing is still fucked up also when
switching a multi tab window from a zone to another the tab list disappears and we cannot drag
only 1 tab it drags the whole window".

- **Zones numbered by place** (5c28852, ADR 0028): `Layout::number_zones` everywhere zones are
  made or change, the editor without a name field, files renumbered once on load (format 1 kept).
  Tests first: 17 core tests asserting numbers failed on the code before (run with the new tests
  on the old sources) and pass; the file test failed before and passes. Core 253.
- **Opening a workspace minimises the other windows on its monitors** (5c28852): the switch test
  with a launcher and the Riot client beside the workspace on its monitor, a window on its other
  monitor and one in full screen failed before (only the workspace left's window minimised) and
  passes.
- **Tab groups below a band** (a14bce2, ADR 0027): the core style test failed before and passes;
  the band-by-reading machinery and the strip's title-bar place removed (core 250 after their
  three tests went with them); the strip put back above a group dropped elsewhere.
- **A workspace shown before it opens, its page's picture and dot rows** (a14bce2, protocol 9,
  `preview_workspace`): interface tests 98, Playwright 28 with axe in dark, light and high
  contrast (both run here, against the stand-in engine); pictures of Home with a workspace
  picked, of a workspace's page and of the zone editor checked by eye.
- **Site and notes** (3cc4db2): the site's desktops wear numbers (1.11.2), the changelog and
  What's new say what changed; the publish test passes.
- **On the owner's PC** (runner started 2026-10-15 21:46 UTC; jobs 0149 to 0151, at 45835e6):
  - Build 1.0.2: 3 min 05 s (Smart App Control refused one build file once; made again), then
    2 min 24 s. `mosaik.ps1 test`: every test passing (interface 98; publish 4, 17 and 3 cases;
    the build with a Mosaïk running, all cases). Pack: 1.4 MB, 275 files.
  - `workspace --drag`, tags in title bars / above windows: saved 21 / 61 ms, opened 47 / 53 ms;
    zones saved as 1, 2, 3; the preview before opening named the window it minimises, and every
    window landed 0 px from its zone and 0 px from where the preview put it; the window always on
    top minimised. Tabs: 1 strip, 2 tabs, chips 81 and 100 px, every tab a chip, the strip in a
    band above; after each of 3 tab clicks the strip above, in the same place, 0 px off; a tab
    pulled out by its chip 0 px from its zone; the group moved to another zone and back with its
    strip above it both times, 0 px; dropped onto the strip, a tab; at the screen's left edge, the
    zone (Windows' snapping on), 0 px. A zone split in the editor: numbered by place, the code
    window 0 px above windows and 28 px in title bars, where it was a tab of a group (the probe
    expected it without its band; it now expects the band of a window in a group). Memory with
    frames: 9.4 and 9.7 MB private.
  - `smart --only arrange`: no check failing; a window put on another in the picture: preview
    3.9 ms, Arrange 20.6 ms, both windows 0 px from where the preview put them, Undo back.
  - UI tour (a profile of its own): 31 pictures; a tile dragged onto another traded places and
    Reset put them back; a workspace picked showed Open, and went back to Arrange.
  - Mosaïk left in the tray on the owner's data: 1.0.2, protocol 9, watchdog, Win+Alt+Space
    registered.
  - **Found**: the new build, asking the Store's 1.0.1 to quit, closed all nine of its windows,
    the one its interface's events go through among them: 1.0.1 logged "asked to close", then
    "the interface thread is gone", and kept running without a way to quit; the build said so
    after 15 s ("An older Mosaïk is running and did not quit"). Fixed below (ADR 0029).

### Asked on 2026-10-15 (16:33): the app reviewed for people new to it, speed and the code

"lets review the app entirely for efficiency, optimizations, and cleanness in order for the use
of the app for complete boomers be quick and effective". Three reviews (25 findings on use, 18 on
speed, 27 on the code); what they found and what changed is ADR 0029.

- **An older Mosaïk asked to quit** through its engine's window alone, and Mosaïk asked to close
  through all its windows (`taskkill`, an installer) quits from its engine side when its
  interface can no longer be reached. Test first: `a_mosaik_asked_to_quit_is_asked_through_its_
  engine_window_alone` (Windows, mosaik-win), waiting for the PC; on the PC, the Store's 1.0.1 as
  the older copy, before (job 0150) and after (job 0152).
- **Mosaïk's window** (c87c397): larger text and 40 px controls, a preview that says so, Back,
  double-click to open, Automatic and five named layouts, results in one place, "On screen",
  "Save as workspace", a suggested set shown before it starts, Settings in the order people use
  them, a first-run line; one read per burst. Request-count tests failed on the code before (3, 3
  and 2 reads where 1 was expected) and pass. Interface 117 tests, Playwright 30 (60 of 60
  repeated); a flaky drag test fixed in the stand-in engine (it forgot the swaps after Arrange).
- **Native words**: Arrange's message in short sentences with the workspace that now opens this
  way; "On screen" in the side menu and the switcher; the side menu's Save row says what it does
  and a save there or in the switcher is confirmed; "save as workspace" in the switcher (and the
  site's switcher, mosaik1.sh 1.11.3).
- **The side menu** ignores a press in the 300 ms after it opened by itself (smart's new step 4b2
  checks that nothing moves and the menu stays).
- **Number tags** whole or compact, never one digit of a bigger number: the core test failed
  before (no compact form) and passes.
- **Speed**: scans of the windows shown only (classify now measures both scans), event-driven
  waits for the move, hide and follow threads, no `DwmFlush` when the switcher opens, previews
  that no longer keep the monitors they read, the Start menu never waited for by Home (suggestions
  follow once it is read), other arrangers looked for at most every 5 minutes.
- **Opening a workspace undone** as Arrange is (the windows it minimised back where they were,
  the workspace open before open again): Home's test failed before (no Undo after Open) and
  passes; the workspace probe's new `undo_open` step waits for the PC.
- **Files**: settings out of range mended on load with one note (the test failed before: 500 px
  kept); damaged workspaces said in words (the test failed before: Rust's debug output).
- **Code**: the strip's title-bar leftovers, a dead field, an untrue notice, the planner's unused
  zone names, public functions nothing called, two unused dependencies, the journal's file name in
  one place, key state read through mosaik-win; ADRs 0011, 0014 to 0019, 0022, 0024 and 0028
  marked as amended; architecture and README brought up to date.
- Checks here: fmt; clippy -D warnings for Windows on every crate; core 251, ipc 11; interface
  119; Playwright 30; the site type-checks; shellcheck.
- **On the owner's PC** (job 0152, at 918e379):
  - Build 2 min 44 s; `mosaik.ps1 test`: every test passing, the new Windows test among them.
  - **The Store's 1.0.1 quit when asked** through its engine's window, 0.25 s after: its log says
    "asked to close", "Mosaïk stopped", then the build "the older Mosaïk quit"; the build ran in
    its place (1.0.2, protocol 9, watchdog).
  - **taskkill without /F did not quit** the build in the tray (still running 20 s later, its log
    silent): taskkill asks only the windows that show, never the engine's. Fixed in 995e0dc (a
    close posted to any window of the engine's thread goes to the engine's), checked by job 0153.
  - **The scan** (classify): 310 top-level windows; the engine's scan 0.6 ms, every window
    3.8 ms (medians of 10).
  - `workspace --drag`, tags in title bars / above windows: saved 6.8 / 43 ms (21 / 61 before),
    opened 24 / 28 ms (47 / 53 before); every window 0 px from its zone and from the preview; the
    window on top minimised; tabs, chips, strip above after clicks and moves, a tab pulled out,
    drops where Windows snaps: as in job 0151; a zone split: 0 px both ways (the code window a tab
    of a group, its band expected). Memory with frames: 9.1 and 9.4 MB private.
  - `smart --only quick`: the side menu opened 51 ms after the pointer rested; a press as it
    opened did nothing (menu open, no window moved). Three checks failed on the probe's side and
    were fixed in it: two read Arrange's old words (every other check of those steps passed), and
    one grabbed the menu's first line before the 300 ms guard was over (now 350 ms after, as a
    person would).
  - UI tour: 31 pictures; Automatic, Undo swaps, Back to my screen and More settings found; a tile
    dragged onto another traded places and was put back; a workspace picked showed Open, and Back
    returned to Arrange. On Windows, the larger type and the controls read as in the pictures
    taken here.

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
- 2026-10-12: Store submission 1 sent for certification with the owner's go-ahead: version
  1.0.0.0, free in all markets, Productivity, rated Everyone / 3+ (IARC), English listing with
  three screenshots of the app, privacy policy `https://xn--mosak-fta.com/conditions`. The first
  upload was refused as a "headless app" (`mosaikctl` was a second app with no Start menu
  entry); it is now an execution alias of Mosaïk's app (ADR 0008). Until the listing is live the
  site's Download buttons lead to Microsoft's "ProductNotFound" page.
- 2026-10-12: Mosaïk must work with practically every app, browser tabs included; tags and tab
  strips go in each window's own title bar, as browsers' tabs do (ADR 0011); tests run each
  app once per run.
- 2026-10-12: every kind of app, Steam, launchers and games included (ADR 0012); one row of
  title bar wherever possible, and where a band is needed above a window, a solid one (ADR 0011).
- 2026-10-12 (1.0.1, first builds): every window starts at the top of its zone ("normalize all
  apps"); zones that follow a resize; Arrange, "very smart", with undo; a quick menu on a line at
  the screen's right edge; at least one vertical preset; no space lost between zones (4 px) and no
  blank band above windows (tags in title bars, a window alone never lowered).
- 2026-10-12 (1.0.1, first builds): Arrange lays every screen out for the task its windows say, the
  main window the most room, in place of the open workspace's zones (ADR 0019); no coloured borders
  and no space between windows (ADR 0020).
- 2026-10-13 (1.0.1, first builds): zones make room for a window too big for them, as little as it
  needs, rather than minimising it or letting it overlap; the window keeps its place in the layout
  and is not made the main one (ADR 0021). Sets of the apps installed that go together, offered on
  the home page, secondary but in view; the windows open the first time sorted into workspaces by
  task; "clean, concise, non-invasive" (ADR 0022). On the site, the header's Download button alone;
  the hero is the title and the desktop.
- 2026-10-13 (1.0.1): "break everything and redo everything", "production ready": the window
  rebuilt, dark by default (ADR 0024). The site's home page as it first was ("Every window, in
  its place.", "Every project gets its own workspace: your apps and windows arranged and ready in
  one click.", "Free and open source."). In the app, one to three zones (four rarely); on the
  site, desktops of two to four windows. Apps shown by their own icons or not at all, and never
  a picture in the middle of a zone. Windows dragged onto each other on Home's picture ("we
  should be able to drag drop apps to permute them"). The site and the app shipped in one line,
  `./mosaik1.sh --deploy --app`, with nothing to answer ("the simplest shipping possible ... dont
  add paperwork for me", ADR 0026); the owner made the Microsoft Entra tenant the Store's keys
  need (`gimz.onmicrosoft.com`; Microsoft blocked the first).
- 2026-10-13: the version stays 1.0 ("rename everything to be v1 not v2 unless i tell you we're
  on v1.0 all the time"): this release is 1.0.1, the lowest version the Store takes after its
  1.0.0, and each next one takes the next third number. The builds were called 1.1 (jobs 0094
  to 0118), then 2.0.0 (jobs 0122 to 0141): what they printed is quoted here as it was.
- 2026-10-13: the Store's key is never written in the scripts ("why should the script asks for
  keys cant you put them in it"): the repository is public and the key alone publishes to the
  Store. The Entra application's tenant and client IDs are in `identity.json`. After the prompt
  failed in Git Bash, the key goes in `.env.local` at the top of the repository, as in a web
  project ("lets just be normal please"), which the owner writes himself (ADR 0026). The owner
  put it instead in his own copy of `mosaik1.sh`, the one beside the repository that no commit
  has, as a line `export MOSAIK_STORE_KEY=...` ("put this shit in the bash file"): `publish.ps1`
  takes it from the environment first. A job that brings that copy up to date keeps the line
  where it is and never prints it.
- 2026-10-14: the site has no section "One desktop, many kinds of work." ("Who it's for", the
  four workspaces as desktops in small): "it looks stacked and is redundant with first section";
  asked before, so it does not come back (ADR 0024).
