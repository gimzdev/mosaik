# Architecture

## Principles

The owner's first rule, above every other trade-off: **minimal, efficient, elegant**.

- **One click to install.** The Download button on mosaïk.com fetches a small installer signed
  by Microsoft, right in the browser (the Store's direct web installer, ADR 0008). It installs for
  the current account only: no administrator prompt, no extra runtimes (the C runtime is linked
  into the binaries; WebView2 ships with Windows 11), no reboot. Mosaïk starts when it is done,
  and Windows keeps it up to date.
- **Easy to use.** One global shortcut, a tray icon, a short first-run welcome. Few settings, each
  with a sensible default.
- **Light.** Measured idle CPU and memory budgets (`docs/STATUS.md`), and nothing running that the
  user did not ask for.
- **Trustworthy.** No telemetry, accounts or cloud; Mosaïk makes no network request at all
  (updates come through the Store). A crash never strands a window. Every failure is explained
  with a way forward.

## Processes

```
mosaik.exe            engine thread: windows, zones, workspaces, tabs, hotkey, the switcher and
  │                   the overlays (native, ADR 0002)
  ├─ mover threads    one per window being moved, so a hung app never holds up the engine
  ├─ hiding thread    hides and shows background tabs (the shell cloak is a call into Explorer)
  ├─ pipe threads     mosaikctl and second launches (ADR 0007)
  ├─ main thread      Tauri: the tray icon, and web windows (settings, editor, welcome) created
  │                   when opened and destroyed when closed
  └─ starts ──────────► mosaik-watchdog.exe   restores hidden windows if mosaik.exe dies (ADR 0006)
mosaikctl.exe         command line client of the pipe
```

The engine thread owns every change Mosaïk makes to other apps' windows, so changes happen in
order on one thread with a Win32 message loop (hotkeys, window events, timers). The interface
and `mosaikctl` send it the same commands (`mosaik_ipc::protocol`); the interface in process,
`mosaikctl` over the pipe. Moving a window is a synchronous call into its app, so the engine
hands each move to a short-lived thread and keeps handling its messages while it waits, at most
2.5 s (`mover.rs`).

The settings window is the same app: its commands reach the engine in process, in protocol
version 3. A save from the zone editor carries the revision of the zones it was opened on, so it
is refused, with the reason, when the monitors or the zones changed meanwhile. While the shortcut
recorder listens, Mosaïk's own shortcuts let go (`hold_hotkeys`, at most a minute), so pressing
one records it instead of opening the switcher.

## How workspaces behave

The decisions are pure functions in `mosaik_core::workspace`, tested on every monitor fixture;
the engine gathers windows and applies the result.

- **Which windows**: those Alt+Tab lists, without an owner, with a title, that Windows lets
  Mosaïk move (not an app running as administrator). Borderless windows that draw their own
  title bar (Steam, launchers, games) count; a popup with no caption, resize frame or minimise
  button (a splash screen, a menu) does not. A game or video in full screen is left alone until
  it leaves full screen (ADR 0012).
- **Saving** (S in the switcher, then a name) turns the windows on screen into a workspace.
  Windows that tile, side by side without overlapping, become zones named after their app;
  windows stacked exactly on top of each other share a zone; maximised windows and windows
  floating over others keep their own place. Windows in zones then fill them, as far apart as
  their two rings are wide (4 px at 100%), so no desktop shows between them. Saving under an
  existing name updates that workspace and keeps the zones its windows sit in.
- **Being in a zone** has one rule everywhere (`holds`): the window's centre is in the zone and
  the window and the zone's window area cover at least 70% of each other. A browser that will
  not shrink to a narrow zone still counts; a small window floating over a zone does not. A
  window without a frame to resize it by (most games) is never resized, only moved: centred
  across its zone, at its top, on the same line as its neighbours; it is in the zone when, along
  each axis, its middle is in the zone or the zone's middle is in it, and it overlaps the zone by
  70% of as much as its size allows (`holds_fixed`). A window that covers two zones stays in the
  one it was put in; with zones that overlap, the nearest middle wins.
- **A layout changes only by the user's hand**: saving, dropping a window into a zone, moving or
  resizing one of the workspace's windows by mouse, Arrange, or the zone editor. Windows an app moved on its own, or a window
  maximised for a moment, go back to their places the next time the workspace opens.
- **Opening** a workspace finds its windows (program, then window class and title to tell
  several windows of one app apart), moves them all at once, starts missing apps, minimises the
  previous workspace's other windows that are on screen, brings its windows forward and gives
  the front one the keyboard. 400 ms later, windows an app moved back are put in place once.
- **Frames**: every window in a zone of the open workspace wears a ring in the zone's colour and
  a tag with its name, drawn right above the window in the stacking order, so windows that
  cover it cover its frame too. Frames follow moves through window events, hooked per process.
- **Tags** (ADR 0011): every window in a zone sits right below a bar at the top of its zone that
  carries its tag (or its group's strip), so windows side by side start at the same line,
  whatever their title bars. The bar is solid and inside the window's ring, so the two read as
  one window, and the window can be dragged by it as by its title bar (Mosaïk moves it with the
  pointer). A setting puts tags in title bars instead: the tag goes in a free stretch of the
  window's own title bar, against its buttons, found by asking the window what is under a row
  of points, as Windows does wherever the pointer goes (`WM_NCHITTEST`), on a thread of its own
  (`titles`); a window whose title bar has no room then sits below a bar, once two readings a
  quarter of a second apart agree.
- **Drag to snap**: while a window is dragged by its title bar, the open workspace's zones show
  under it; the zone under the pointer (or within 48 px) is tinted, and the window is pulled into
  it on release. Shift moves freely; a setting swaps that. A window that was not part of the
  workspace joins it. A resize never snaps: Windows shows its resize arrows for the whole of one
  (ADR 0010).
- **Zones follow a resize** (ADR 0013): resizing one of the workspace's windows in a zone by an
  edge it shares with other zones moves the line between them, the zones on both sides and their
  windows with it; the zones show as they will be while the resize lasts. An app that keeps a
  larger size than its new zone pushes the line back, once. An edge on the monitor's border has
  no line: the window goes back to its zone. Shift resizes freely.
- **Arrange** (ADR 0014): one command puts every window on screen in the zone that suits it, by a
  pure planner (`mosaik_core::arrange`): zones named after an app or a kind of app draw those
  windows, the windows used most get the largest zones, windows stay where they are when they
  fit, and windows left over become tabs. A monitor without zones gets a preset for its windows;
  with no workspace open, a new one is made. Undo puts everything back, for ten minutes.
- **The quick menu** (ADR 0015): a thin line on an edge of the main monitor opens, when the
  pointer rests on it, onto Arrange, the switcher, saving, the zone editor, snapping and the
  settings, without taking the keyboard. It hides while a game or a video fills the main monitor.
  Dragged along its edge, it stays there. Apps that drag their window themselves, without Windows' move loop, are followed
  too: a window that took the mouse and moves at the same size is being dragged. A window Mosaïk must not move (an app running as administrator, or one Windows
  protects) gets no zones, and the user is told why once, when the drag ends.
- **Living with other arrangers** (ADR 0010): Windows' own snapping stays on; a drop on a zone or
  a tab at the edge of the screen ends where the user dropped it, and the first one tells the
  user why Windows showed its preview too. Other programs that arrange, snap or group windows
  (komorebi, GlazeWM, FancyZones...) are named once, ever, when Mosaïk starts or opens a
  workspace while one runs, with what to do. Mosaïk never changes their settings or Windows'.
- **Displays changing** (a monitor plugged in or out, a scale or taskbar change, waking up) fits
  the open workspace's windows to the new arrangement: zones are fractions of a monitor's work
  area, and each remembered monitor is matched to one connected now.

## How tabs behave

A tab group is several windows of any apps sharing one place, one visible at a time, with a
strip in the bar above the visible tab (or in its title bar, with that setting, ADR 0011): the
zone's tag, then one chip per tab. Mosaïk never reparents another app's window; background tabs are hidden with the
shell's cloak (ADR 0003).

- **Making a group**: drag a window by its title bar onto another window's tag, or onto a
  group's strip; a "Drop to add" slot shows where it will go. It takes the group's place and
  shows.
- **Using it**: click a chip to show that tab; drag a chip along the strip to reorder; drag it
  away from the strip to pull it out (zones show, as when dragging a window; Shift moves
  freely; Esc cancels). A crowded strip shrinks its chips, then gathers the rest in a "+N" chip.
- **One rule decides what shows** (`mosaik_core::tabs::plan_tabs`, applied by `sync_tabs` after
  anything that changes groups): a window stays hidden only while it is a background tab of a
  group of the open workspace whose other tabs can be reached, through its strip, or through
  its visible tab's taskbar button while that one is minimised or maximised. Every other window
  Mosaïk hid comes back: in its place when the open workspace uses it, minimised otherwise.
  When no strip can be drawn (no room, drawing failed) or a window will not hide, the group shows
  all its tabs, stacked, and the user is told once.
- **Safety**: every window is written to the journal, in one write per batch, and marked before
  it hides, and only while the watchdog runs (it is started again if it stops). Hiding and
  showing run on their own thread with a deadline (ADR 0009); a window that does not come back
  stays in the journal and is retried, and "Rescue windows" brings back everything.
- **Apps and tabs**: a tab whose app hides its window (closes to the tray) or closes it gives
  its place to the tab on its right, else on its left. A background tab an app brings forward
  (a notification, a link) shows for now; the layout keeps the user's choice.
- **Layouts**: groups, their tab order and the tab that shows are part of the workspace, saved
  where the visible tab is. With tags above windows, a group's windows sit below a band with
  room for the strip even when tags are hidden. Opening a workspace moves only the tabs that
  show; a tab goes to its group's place when it is shown.

## Crates

| Crate | What it is |
| --- | --- |
| `mosaik-core` | Pure logic, no Win32: geometry, topologies, zones, layouts, matching, classification, snapping, placement maths, tabs, the journal, settings and the store. Tested on every platform. |
| `mosaik-win` | The Win32 adapter: enumerating and classifying windows, placing them by their visible frame, hiding them, monitors and DPI, hooks, hotkeys, processes, rescue. Decides nothing. |
| `mosaik-overlay` | Native overlays drawn with Direct2D in software: rings as four thin windows, name tags, the drop-target tint, tab strips, the quick menu. |
| `mosaik-engine` | The engine thread, its commands, the pipe server and the watchdog launcher. |
| `mosaik-ipc` | The versioned protocol, the named pipe and `mosaikctl`. |
| `mosaik-watchdog` | The watchdog process. |
| `mosaik-probe` | The measurements behind the ADRs, re-run before every release. Not shipped. |
| `apps/desktop` | `mosaik.exe`: Tauri shell and the React interface. |

The brief's layout lists five crates; `mosaik-engine` is added so the engine can be tested and
measured without the interface, and the Tauri crate stays a thin shell.

## Two files

The repository travels as two files that each run on their own on another machine:
`mosaik.ps1` (the app: its source inside, with check, build, test, run and package steps, for
Windows PowerShell 5.1 and later; `pack` makes the one-file copy, `verify` checks it) and
`mosaik1.sh` (the website: it writes `site/`, installs it, runs it or deploys it). CI runs the
same `mosaik.ps1` steps the owner does, and checks that `site/` is exactly what `mosaik1.sh`
writes.
