# Architecture

## Principles

The owner's first rule, above every other trade-off: **minimal, efficient, elegant**.

- **One click to install.** The Download button on mosaïk.com fetches a small installer signed
  by Microsoft, right in the browser (the Store's direct web installer, ADR 0008). It installs for
  the current account only: no administrator prompt, no extra runtimes (the C runtime is linked
  into the binaries; WebView2 ships with Windows 11), no reboot. Mosaïk starts when it is done,
  and Windows keeps it up to date.
- **Easy to use.** One global shortcut, a tray icon, a side menu at the screen's edge, and a home
  page that shows the screen as a layout will leave it before anything moves. No tour, no
  pop-ups: what Mosaïk has to say while it works fades in a toast and waits behind a bell. Few
  settings, each with a sensible default; dark by default.
- **Light.** Measured idle CPU and memory budgets (`docs/STATUS.md`), and nothing running that the
  user did not ask for.
- **Trustworthy.** No telemetry, accounts or cloud; Mosaïk makes no network request at all
  (updates come through the Store). A crash never strands a window. Every failure is explained
  with a way forward.

## Processes

```
mosaik.exe            engine thread: windows, zones, workspaces, tabs, hotkey, the switcher and
  │                   the overlays (native, ADR 0002)
  ├─ mover threads    one per window being moved, so a hung app never holds up the engine;
  │                   one following a window dragged by its band (`mosaik-follow`)
  ├─ hiding thread    hides and shows background tabs (the shell cloak is a call into Explorer)
  ├─ titles thread    reads windows' title bars, where their tags go (`mosaik-titles`)
  ├─ Start menu       reads the apps installed, for the sets of apps (`mosaik-start-apps`)
  ├─ launch threads   start a workspace's or a set's apps that are not open (`mosaik-launch`)
  ├─ saver thread     writes workspaces.json once changes stop (`mosaik_core::saver`)
  ├─ pipe threads     mosaikctl and second launches (ADR 0007)
  ├─ main thread      Tauri: the tray icon, and Mosaïk's window (Home, a workspace's page, the
  │                   zone editor, Settings), created when opened and destroyed when closed
  ├─ icon thread      reads apps' icons for the window, once per app (`mosaik_win::icons`)
  └─ starts ──────────► mosaik-watchdog.exe   restores hidden windows if mosaik.exe dies (ADR 0006)
mosaikctl.exe         command line client of the pipe
```

The engine thread owns every change Mosaïk makes to other apps' windows, so changes happen in
order on one thread with a Win32 message loop (hotkeys, window events, timers). The interface
and `mosaikctl` send it the same commands (`mosaik_ipc::protocol`); the interface in process,
`mosaikctl` over the pipe. Moving a window is a synchronous call into its app, so the engine
hands each move to a short-lived thread and keeps handling its messages while it waits, at most
2.5 s (`mover.rs`); the wait ends as soon as the last thread has answered (an event it sets).
Scans read the windows Windows shows (ADR 0029): the rest, nine in ten top-level windows, are
never Mosaïk's to manage.

Mosaïk's window is the same app: its commands reach the engine in process, in protocol
version 9. A save from the zone editor carries the revision of the zones it was opened on, so it
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
  Windows that tile, side by side without overlapping, become zones;
  windows stacked exactly on top of each other share a zone; maximised windows and windows
  floating over others keep their own place. Windows in zones then fill them, edge to edge, so no
  desktop shows between them (ADR 0020). Saving under an existing name updates that workspace
  and keeps the zones its windows sit in (Arrange's, when they are on screen: ADR 0030).
- **Being in a zone** has one rule everywhere (`holds`): the window's centre is in the zone and
  the window and the zone's window area cover at least 70% of each other. A browser that will
  not shrink to a narrow zone still counts; a small window floating over a zone does not. A
  window without a frame to resize it by (most games) is never resized, only moved: centred
  across its zone, at its top, on the same line as its neighbours; it is in the zone when, along
  each axis, its middle is in the zone or the zone's middle is in it, and it overlaps the zone by
  70% of as much as its size allows (`holds_fixed`). A window that covers two zones stays in the
  one it was put in; with zones that overlap, the nearest middle wins.
- **A layout changes only by the user's hand**: saving, dropping a window into a zone, moving or
  resizing one of the workspace's windows by mouse, or the zone editor. Windows an app moved on
  its own, or a window maximised for a moment, go back to their places the next time the
  workspace opens.
- **Arrange arranges, Save saves** (ADR 0030): Arrange makes and changes no workspace. The zones
  it puts on screen are kept apart (`Arranged`, id `~arranged`), and tags, snapping, tabs and the
  changes above go to them, never to the disk, until a workspace is opened or saved; the
  workspace in use stays the one Save updates. A save keeps Arrange's zones
  (`mosaik_core::workspace::saved_over`): all of them saved over a workspace, the ones a window
  is in for a new one.
- **Zones are numbered** 1, 2, 3… in reading order, monitor by monitor, row by row from the
  top, then from the left (`Layout::number_zones`, ADR 0028): a zone's name says where it is,
  never what it holds. Every change of zones numbers them again; a file with names from before
  is renumbered once on load.
- **Opening** a workspace finds its windows (program, then window class and title to tell
  several windows of one app apart), moves them all at once, starts missing apps, minimises the
  other windows on screen on the monitors it is on (and the previous workspace's anywhere; not
  a window in full screen; ADR 0028), brings its windows forward and gives the front one the
  keyboard. 400 ms later, windows an app moved back are put in place once.
- **Frames**: every window in a zone of the open workspace wears a tag with its zone's number and
  colour, and, when the user turns rings on, a ring in that colour (then windows sit two rings
  apart; ADR 0020). Both are drawn right above the window in the stacking order, so windows
  that cover it cover them too, and follow moves through window events, hooked per process.
- **Tags** (ADR 0011): the tag goes in a free stretch of the window's own title bar, against its
  buttons, found by asking the window what is under a row of points, as Windows does wherever
  the pointer goes (`WM_NCHITTEST`), on a thread of its own (`titles`). Every window fills its
  zone and starts at its top: a tag with little room is drawn compact, its whole number without
  its side padding (`zone::tag_cut`), and a window with no room even for that wears none. A tab group's visible
  tab sits below a band with the group's strip in it (ADR 0027). A setting puts every tag in a
  bar at the top of its zone instead, above every window. A band is solid and,
  with rings on, inside the window's ring, so the two read as one window; the window can be
  dragged by it as by its title bar (Mosaïk moves it with the pointer).
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
- **Arrange** (ADR 0014, 0019): one command lays out every monitor with windows for the task its
  windows say (`mosaik_core::task`: coding, design, writing, browsing, chat and mail, games), by
  a pure planner (`mosaik_core::arrange`). The window the task is done in gets the most room, a
  terminal goes under the editor when coding, and the windows that help most sit beside it; the
  others stack in the zones, behind a window of their kind or where there are fewest (ADR 0032;
  before, they were minimised, ADR 0017). Its zones replace the open workspace's on that monitor; with
  no workspace open, a new one is made. No window gets a zone smaller than its app lets it go
  (`window::least_frame`, from `WM_GETMINMAXINFO`). Arranging twice moves nothing: the zones
  Move windows made are the layout again while every window is still in its zone, whatever task
  the windows say then (step 0 of the plan, `KeptZone::arranged`, ADR 0032), and each zone takes
  the colour its window wears (`arrange::zone_colors`). Undo puts everything back, for ten
  minutes.
- **Zones make room** (ADR 0021): a zone too small for its window grows just enough, and the
  zones beside it give up the room, each keeping what its own windows need and at least
  320 x 200 (`mosaik_core::fit`, which reads zones that tile a monitor as a tree of cuts and
  moves them as little as it can). The window keeps its place in the layout; it is not made the
  main one. Arrange fits every layout it considers this way, a layout picked does it after
  putting each window in a zone, and the engine does it from the size a window kept after
  opening a workspace, a drop or a display change (`room.rs`). A window that cannot fit beside
  the others stacks behind one; it is minimised only when it fits no zone (ADR 0032).
- **Windows move only on the user's actions** (ADR 0017): Arrange, a layout, opening or saving
  a workspace, a drop, the zone editor, a resize by hand, a tab clicked, an app added or moved.
  A title bar read again later (a window brought forward) moves no window; a tab group's strip
  sits in the band above its visible tab (ADR 0027).
- **A workspace's apps** (ADR 0018, 0023, 0024, 0028, 0031): Home shows them by their icons, and
  a workspace's page shows how it opens (`preview_workspace`) above its apps, zone by zone: each
  zone's number on its colour, a line of its colour down its apps, an empty zone saying so; each
  app with Move (the other zones) and × to take it out. On its picture, an app dragged onto a
  zone goes in it, onto another app the two trade zones (`move_app` with `swap_with`,
  `editing::swap_apps`), or in one zone their places in it, front to back or in their tab strip
  (`editing::trade_in_zone`; a zone's apps rank front to back by `Layout::ranks`, which opening
  it follows, and the open workspace's windows are stacked so at once, ADR 0032), and the lines
  between zones save its zones (`zones`, `set_zones`, only the edges moved changing). Its list of
  apps takes the same drops (`AppsList.tsx`, ADR 0033). Add an app offers every app it could take
  in one list: the windows open now and the Start menu's apps that are not open (`installed`,
  `add_installed_app`, `sets::startable`), the latter starting when it opens. Adding an app,
  moving one to another zone or removing it is a pure edit (`mosaik_core::editing`), saved, then
  applied at once in the open workspace. A layout chosen on Home while it is shown lays it out
  (`set_workspace_layout`, `editing::with_layout`: every app kept in a zone), with Undo.
- **Home** (ADR 0016, 0024, 0031): a picture of the main monitor as the layout chosen would leave
  it, drawn from `preview_arrange` (the same preparation as Arrange, changing nothing): Automatic
  first (Arrange's own layout for the task it reads), or one of eight layouts, all in view in two
  rows; the windows it would minimise are named under it (only those that fit no zone: the rest
  stack in the zones, ADR 0032). The picture (`Screen.tsx`) draws each
  zone once, numbered and coloured, empty ones too, with the title bars of the windows it takes
  (Mosaïk says which: each preview zone's `id`, each window's and tab's `zone` and `app`). A
  window dragged into a zone joins it, behind its windows, moving no other (ADR 0033); dragged
  onto another, the two trade places (a click or Enter picks one, a second puts it there):
  exactly the two dropped one onto the other (`onto`), zones and places front to back, own window
  or stacked; Arrange lists and stacks each zone's windows front to
  back (`Plan::order`, its own window first; `mover::stack` moves a window down only when it is
  in front of the one before it, and Undo puts it back), and keeps an arranged screen as it is
  and windows already in a zone as they are on screen (of those filling a zone, the one in front
  is its own); the lines
  between zones resize them (`LineHandle`, hidden until pointed at). The moves and the areas go
  with the preview and with Arrange (`mosaik_core::arrange::plan_moved`, the areas as its
  `Stretched` zones, taken after the windows are found in the zones so that a line moves no
  window to another zone, ADR 0033), each zone keeping its number and its colour (the colours of
  the picture without the moves, but for two zones whose own windows trade places, which trade
  theirs: `Plan::unmoved`). Move windows does it (ADR 0032),
  changing no workspace, and Undo brings everything back; what was changed stays chosen until
  Move windows makes it (then the screen holds it, and the picture shows the screen as it is),
  and Undo changes puts the layout's own back. Beside it, every workspace with its apps' icons: a
  click shows it in the picture as opening it would leave the screen (`preview_workspace`, the
  plan opening it follows: the apps it would start drawn dashed, a tab group as one bar of its
  tabs, the windows it would minimise named below), with Open in place of Move windows (ADR
  0028), and its apps and zones changed on it are saved in it at once (`workspacePicture.ts`);
  a double-click opens it, and Back, Esc or a layout chosen goes back to the screen. The title
  row, the picture and the layouts under it are one width (`pictureWidth`). A ⋯ menu has Open
  and the rest, then come
  the suggested sets, shown the same way before "Start these apps" (ADR 0029). What an action did
  shows under the picture. The window reads from the engine once per burst (`reads.ts`: focus, a
  change and its own action make one read 150 ms after the last). Mosaïk's window has two
  places, Home and Settings (with About at its end).
- **Apps' icons** (ADR 0024): replies name each app's icon by a key (`app:<AppUserModelID>` or
  `exe:<path>`, `mosaik_core::icons`); the window asks for the ones it shows in one batch
  (`app_icons`), read once per app as 48 px PNGs by the shell's image factory on a thread of their
  own. An app without one shows no picture.
- **Toasts and notes** (ADR 0024): what the engine has to say while it works (`Engine::notice`)
  shows in a native toast by the corner of the screen the pointer is on (`mosaik_overlay::toast`),
  one at a time, never taking the keyboard, and goes to the window's notes; Mosaïk's window never
  opens by itself.
- **Sets of apps** (ADR 0022): the apps of the Start menu (read from the shell's Apps folder on a
  thread of their own, kept ten minutes) that go together for a task, chosen by
  `mosaik_core::sets`, are offered on the home page. A click opens the workspace that has the
  set's apps, or makes one laid out for its task (three windows to a view, `task::MOST_IN_VIEW`;
  the set's other apps as tabs with the app they go with, `task::tabs_for`), starting the apps
  that are not open. An app open is used (packaged apps' windows take their process's
  AppUserModelID, `AppMatch::same_app`); a start still pending when the workspace is opened again
  stands for its slot; a started app is placed when its window shows or uncloaks (ADR 0023). The
  first time Mosaïk's window opens, the windows open are sorted into a workspace per task,
  without moving any and without a word: the workspaces made are in Home's column, and a sorting
  that failed says why in Mosaïk's notes (ADR 0033).
- **The side menu** (ADR 0015, 0017, 0023, 0024): a thin line on an edge of the main monitor
  slides open as soon as the pointer stops on it, onto Arrange, five layouts drawn in small, the
  workspaces with the one open marked, saving, the zone editor and snapping, without taking the
  keyboard. It sits on the
  edge chosen if the pointer stops there, else on the other (`mosaik_core::edge`), and does not
  answer while an app confines the pointer, or while the window in front, covering the main
  monitor, hid it the last time it was seen over it, as games do (`Engine::pointer_tick`). It hides while a game or a video fills the main monitor.
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
strip in a band above the visible tab (ADR 0027): the zone's tag, then one chip per tab, each
with room to be dragged out. Mosaïk never reparents another app's window; background tabs are
hidden with the shell's cloak (ADR 0003).

- **Making a group**: drag a window by its title bar onto another window's tag, or onto a
  group's strip; a "Drop to add" slot shows where it will go. It takes the group's place and
  shows.
- **Using it**: click a chip to show that tab; drag a chip along the strip to reorder; drag it
  away from the strip to pull it out (zones show, as when dragging a window; Shift moves
  freely; Esc cancels). A crowded strip shrinks its chips, then gathers the rest in a "+N" chip.
  The strips and frames are put back above their windows 120 ms and 400 ms after a chip is
  clicked, for apps that raise their window again once it shows (ADR 0023).
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
  where the visible tab is. A group's windows sit below a band with room for the strip, whether
  tags are in title bars, above windows or hidden (ADR 0027). Opening a workspace moves only the tabs that
  show; a tab goes to its group's place when it is shown.

## Crates

| Crate | What it is |
| --- | --- |
| `mosaik-core` | Pure logic, no Win32: geometry, topologies, zones, layouts, matching, classification, snapping, placement maths, Arrange and its tasks, zones making room, sets of apps, tabs, the journal, settings and the store. Tested on every platform. |
| `mosaik-win` | The Win32 adapter: enumerating and classifying windows, placing them by their visible frame, hiding them, monitors and DPI, hooks, hotkeys, processes, the Start menu's apps, rescue. Decides nothing. |
| `mosaik-overlay` | Native overlays drawn with Direct2D in software: rings as four thin windows, name tags, the drop-target tint, tab strips, the side menu, toasts. |
| `mosaik-engine` | The engine thread, its commands, the pipe server and the watchdog launcher. |
| `mosaik-ipc` | The versioned protocol, the named pipe and `mosaikctl`. |
| `mosaik-watchdog` | The watchdog process. |
| `mosaik-probe` | The measurements behind the ADRs, re-run before every release. Not shipped. |
| `apps/desktop` | `mosaik.exe`: Tauri shell and the React interface. |

The brief's layout lists five crates; `mosaik-engine` is added so the engine can be tested and
measured without the interface, and the Tauri crate stays a thin shell.

## Files

Settings, workspaces and the journal of hidden windows are in `%APPDATA%\Mosaik`, the log in
`%LOCALAPPDATA%\Mosaik\logs` (`mosaik_core::store::data_folder`, `logs_folder`). For tests,
`MOSAIK_DATA` names another folder for both: a Mosaïk with files of its own that still sees the
account's Start menu. A program started with `APPDATA` and `LOCALAPPDATA` changed sees only its
packaged apps there (35 of 139 on the owner's PC, for Windows' own `Get-StartApps` too; job
0111).

## Two files

The repository travels as two files that each run on their own on another machine:
`mosaik.ps1` (the app: its source inside, with check, build, test, run and package steps, for
Windows PowerShell 5.1 and later; `pack` makes the one-file copy, `verify` checks it) and
`mosaik1.sh` (the website: it writes `site/`, installs it, runs it or deploys it). CI runs the
same `mosaik.ps1` steps the owner does, and checks that `site/` is exactly what `mosaik1.sh`
writes.
