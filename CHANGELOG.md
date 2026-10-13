# Changelog

## 2.0.0

Mosaïk 2 is a new window on the same engine, with everything the 1.1 builds added since 1.0
(below).

### A new window

- Dark by default. A theme left as 1.x set it (following Windows) becomes dark once; a theme you
  picked stays. Light and "Like Windows" are a click away in Settings.
- Home shows your screen as it will be: each window in its zone, with its app's icon, its name
  and its zone's tag, a thin line of desktop apart. Smart is chosen first, Arrange's own layout
  for what you are doing ("Smart layout for coding"); the ten layouts are drawn beside it. Choose
  one and the picture changes before anything moves, with the windows it would minimise named
  under it. Arrange does it; Undo puts everything back.
- Your workspaces sit beside the screen, each with its layout in small, its apps' icons and its
  shortcut: one click opens one. Suggested sets of the apps you have that go together are under
  them. Save layout keeps what is on screen as a new workspace.
- Apps are shown by their own icons, read from Windows. An app without one shows none.
- No more pop-ups or banners. What Mosaïk has to tell you while it works (another app that moves
  windows, a window it cannot move, a shortcut taken) shows in a small toast by the corner your
  pointer is on, and fades; it also waits behind the bell at the top of Mosaïk's window. A
  problem with something you just did is said next to it.
- A workspace's own page: its apps and the zone each goes in, its name, its shortcut, its zones,
  and Delete.
- Settings: General (the switcher's shortcut, starting with Windows, the theme), Side menu,
  Windows (snapping, zone tags, coloured borders), Advanced, and About.
- The quick menu is the side menu: Arrange, five layouts drawn in small, your workspaces with the
  one open marked, Save layout, Edit zones and Snap while dragging. It, the switcher and the
  toasts share the window's colours. The switcher names each workspace by its apps.
- No welcome to click through. The first time, the windows you have open are sorted into
  workspaces, and Mosaïk installed from the Store starts with Windows (Settings turns that off).
- Settings saved by a newer Mosaïk are read as well as they can be, shown, and never written
  over.

## 1.1.0 (development builds, not released on their own)

### A home page that shows before it moves

- Mosaïk opens on a picture of your main screen and its windows. Point at Arrange windows,
  or at one of ten layouts (halves, thirds, main and side, quarters, rows...), and the windows
  glide to where they would go; click, and they do. Undo puts everything back.
- A layout replaces the zones of your main screen; windows more than its zones are minimised.
- Open the apps for coding, design, writing or games in one click: under Arrange, a button for
  each set of the apps you have that go together, such as your editor, a terminal, a browser,
  an assistant and File Explorer for coding. It opens them as a workspace laid out for the task,
  three windows side by side at most and the others as tabs with the app they go with, and
  starts the ones that are not open: an app already open is used, never started twice, and each
  one takes its place as soon as its window shows. Nothing shows when Mosaïk finds no set.
- Your workspaces are a click away below, each with Open; its ⋯ menu changes its apps, edits its
  zones, gives it a shortcut, renames or deletes it. The welcome shows Arrange at work before
  you start.
- Two places only: Home and Settings, in a bar at the top. Settings shows the few choices most
  people change; the rest are under More settings, and About is at the end.
- A menu near the bottom of the window opens upward, so all of its items show.
- The tray menu's and the quick menu's Settings is now Open Mosaïk. A click on the tray icon
  opens it too, and so does opening Mosaïk from the Start menu; started with Windows, it waits
  in the tray.

### Every window at the top of its zone

- A window whose title bar has little room for its zone's tag no longer sits lower than its
  neighbours: it stays at the top of its zone, and its tag is cut short, then to the zone's
  initial in a small round chip.
  A tab group's tabs sit at the right end of its title bar, in the same place on every
  window. Every window fills its zone; no band of empty space sits above any of them.
- A window that cannot be resized keeps its size centred across its zone and at its top, on the
  same line as its neighbours, no longer in the middle.
- Windows meet edge to edge, without coloured rings: no space is lost between them or at the
  screen's edges. Settings, Coloured borders, brings the rings back, with a little space between
  windows. Settings from 1.0 move to it once; a space you chose yourself stays.

### Zones that follow a resize

- Resize a window in a zone by an edge it shares with other zones, and the line between them
  moves with it: the zones on both sides, and their windows, follow. While you resize, the zones
  show as they will be. Hold Shift to resize the window alone; a setting turns it off.
- An app that will not get as small as its new zone (a browser keeps about 500 px) pushes the
  line back, so windows never overlap.

### Zones that make room

- A window that will not get as small as its zone (Battle.net keeps about 1280 x 720) gets a
  zone just big enough for it: the zones beside it give up the room, as little as they can, and
  the window keeps its place in the layout. With Arrange, with every layout, when a workspace
  opens and when you drop a window in a zone. A window waits in the taskbar only when the
  windows cannot all fit on the screen.

### Arrange

- One click lays out every screen for what you are doing: from the home page, the tray menu, the
  quick menu or `mosaikctl arrange`. Arrange reads the task from your windows (coding, design,
  writing, browsing, chat and mail, games). The window you work in gets the most room, a
  terminal goes under your editor, and the windows that help sit beside it, three windows to a
  screen at most. The rest are minimised and wait in the taskbar. On an ultrawide screen the main window is in the middle; on
  one turned on its side, at the top. The home page says which task it read.
- The first time you start Mosaïk, the windows you have open are sorted into workspaces, one
  for each task, each laid out for it. No window moves.
- Arrange replaces the open workspace's zones on the screens it lays out, and arranging again
  moves nothing. With no workspace open, Arrange makes a new one.
- Undo puts every window back where it was, for ten minutes.

### Windows move only when you ask

- Mosaïk moves or resizes a window only when you ask it to: Arrange, a layout, opening or
  saving a workspace, a drop, the zone editor. Bringing a window forward (Alt+Tab) or opening
  one never moves it.

### Your workspaces' apps

- The home page lists each workspace's apps; Change its apps… in its ⋯ menu shows the zone each
  goes in: pick another zone, remove an app, or add a window that is open. In the open
  workspace, the window moves at once.
- An app a workspace or a set starts opens in its own folder, as from File Explorer, never in
  the folder Mosaïk was started from (Steam wrote its logs there).
- Apps have proper names: a Store app is called by its own name ("Calculator", not
  "ApplicationFrameHost"), and Explorer is "File Explorer".

### The quick menu

- A thin line at the right edge of the main screen. Point at it and it slides open: Mosaïk and
  your open workspace (click to open Mosaïk), Arrange, the switcher, Save layout, Edit zones,
  snapping on or off. It never takes the keyboard from the app you are typing in, steps aside
  for a game or a video in full screen, and stays where you drag it along the edge. Settings:
  right edge, left edge, or off.
- When another screen or the taskbar is beyond the edge you chose, the pointer does not stop
  there: the line goes on the other edge, where it does, and Mosaïk says so once, the next time
  you open it.
- It never opens for a game: while a game hides the pointer to turn the view with the mouse, the
  line does not answer. In the game's menus, with the pointer showing, it does.

### Tabs

- Clicking a tab no longer lets the window shown cover the tab strip: the strip stays on top.

### Zone editor

- Four more presets: a half beside two stacked zones, its mirror, two rows and three rows.

### Under the hood

- `mosaikctl state` lists the quick menu, where it is and its rows. `mosaikctl arrange <layout>`
  uses a layout, and `mosaikctl preview [<layout>]` says what Arrange would do without doing it.
  `mosaikctl apps`, `add-app`, `move-app` and `remove-app` list and change a workspace's apps.
  `mosaikctl sets`, `open-set` and `sort-windows` list the sets of apps, open one and sort the
  windows open into workspaces. The protocol is version 6; settings are format 5, and settings
  from 1.0 move to it on their own.
- The app and the site say less: shorter pages, the essentials only.

## 1.0.0

The first release. Mosaïk gives every project its own workspace: your apps and windows arranged
and ready in one click. Free, open source (CC0), no account, no telemetry, no network requests.

### Zones and saved layouts

- Save the windows on screen as a workspace (S in the switcher): windows side by side become
  named, coloured zones; windows stacked exactly on top of each other share one.
- Every window in a zone wears a ring in its colour and a tag with its name, in its own title
  bar next to its buttons, so windows fill their zones. A window whose title bar has no room
  wears its tag above it; a setting puts every tag above its window.
- Drag a window by its title bar and the open workspace's zones show; let go and it is pulled
  into the zone under the pointer. Shift moves freely (a setting swaps that).
- The zone editor: drag or arrow-key the lines between zones, split, rename, recolour, remove.

### Tabs for any app

- Drop a window on another window's tag, or on a group's strip, and the two become tabs of one
  place: one shows, the others wait, out of sight. The strip sits in the visible window's title
  bar, as browsers' tabs do, maximised windows included.
- Click a tab to show it, drag it along the strip to reorder, drag it away to make it a window
  again. Tab order is kept with the workspace.
- Tabs dragged out of Chrome or Edge join in like any window.
- Checked end to end with Notepad, Notepad++, Word, Excel, Explorer, Windows Terminal, VS Code,
  Chrome, Edge, Calculator, Paint and Git Bash.

### Every kind of app

- Apps that draw their own title bar in a borderless window, such as Steam and game launchers,
  and games in a window join zones, tabs and workspaces like any other window.
- A window that cannot be resized, as most games' windows, keeps its size: Mosaïk only moves it,
  to the middle of its zone.
- A game or a video in full screen is left alone until it leaves full screen.
- Where a window's title bar has no room for its tag or tabs, they sit on a solid band right
  above it, inside its ring, so the two read as one window; the band works as its title bar:
  click it to bring the window forward, drag it to move the window.

### Switching

- One shortcut, Win+Alt+Space, opens the switcher: a preview of each workspace; 1 to 9, then F1
  and up, to pick; arrows and Enter, typing to filter, S to save, Esc to close.
- The tray menu lists the workspaces too; any workspace can have a shortcut of its own.
- Positions, sizes and tabs come back together; apps that are not running are started. Eight
  windows switch in about 80 ms.

### Every monitor

- Zones are kept as proportions of a monitor and fit again when monitors, their scale or the
  taskbar change; each window lands on a monitor that is there.

### Mosaïk's window

- A welcome on the first run, then workspaces, the zone editor, settings and about, in light,
  dark or as Windows has it, contrast themes included.
- Every control can be reached with the keyboard, which goes round the window and stays in an
  open question, and every one has a name for screen readers.

### Safe with your windows

- A separate watchdog brings back every hidden window if Mosaïk stops unexpectedly, and
  "Rescue windows" in the tray brings back everything at any time.
- Windows of apps running as administrator are left alone, and Mosaïk says why.
- Windows' own snapping stays on; a drop on a zone or a tab ends where you dropped it.
  komorebi, GlazeWM, PowerToys FancyZones and similar tools are named once if they run beside
  it, with what to do.

### Requirements

- Windows 11, or Windows 10 version 22H2, on a 64-bit (x64) PC. Installed and kept up to date by
  the Microsoft Store.

### Known limitations

- Firefox makes the window for a dragged-out tab only when the tab is dropped: drag that window
  into a zone or onto a tag afterwards.
- A window that Windows' snapping also arranged keeps Windows' mark: dragged again, Windows
  first gives it back its earlier size.
- Mosaïk reads where a title bar is free from what the app says about it. An app that changes
  its title bar without a new title while it stays in front (a browser opening a tab in the
  background) gets its tag moved when it next comes to the front or changes size.
- Measured on Windows 11 with a 1920x1080 monitor at 100% scale, alone and beside a second
  display at the same scale. Two monitors with different scales, sleep and resume, monitors
  plugged in and out, Remote Desktop and Windows 10 are covered by tests of the logic, not yet
  by runs on such machines.
