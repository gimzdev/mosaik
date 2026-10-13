# Changelog

## 1.1.0

### A home page that shows before it moves

- Mosaïk opens on a picture of your main screen and its windows. Point at Arrange windows,
  or at one of ten layouts (halves, thirds, main and side, quarters, rows...), and the windows
  glide to where they would go; click, and they do. Undo puts everything back.
- A layout replaces the zones of your main screen; windows more than its zones become tabs.
- Your workspaces are a click away below, and the welcome shows Arrange at work before you
  start.
- The tray menu's and the quick menu's Settings is now Open Mosaïk. A click on the tray icon
  opens it too, and so does opening Mosaïk from the Start menu; started with Windows, it waits
  in the tray.

### Every window at the top of its zone

- A window whose title bar has little room for its zone's tag no longer sits lower than its
  neighbours: it stays at the top of its zone, and its tag is cut short, then to the zone's
  initial in a small round chip; with no room even for that, it wears its coloured ring alone.
  Only the tab that shows of a group with no room for its tabs sits below a bar, so the tabs
  stay in reach. Every window fills its zone; no band of empty space sits above any of them.
- A window that cannot be resized keeps its size centred across its zone and at its top, on the
  same line as its neighbours, no longer in the middle.
- Zones sit closer together: windows are as far apart as their coloured rings are wide, so no
  desktop shows between them (it was 8 px, now 4 px at 100%). Settings from 1.0 move to it
  once; a space you chose yourself stays.

### Zones that follow a resize

- Resize a window in a zone by an edge it shares with other zones, and the line between them
  moves with it: the zones on both sides, and their windows, follow. While you resize, the zones
  show as they will be. Hold Shift to resize the window alone; a setting turns it off.
- An app that will not get as small as its new zone (a browser keeps about 500 px) pushes the
  line back, so windows never overlap.

### Arrange

- One click puts every window on screen in the zone that suits it: from the home page, the tray
  menu, the quick menu or `mosaikctl arrange`. Zones named after an app or a kind of app
  ("Browser", "Terminal") draw those windows; the windows you use most get the largest zones;
  windows already in a zone stay there; windows more than the zones become tabs.
- A monitor without zones gets a layout for its windows: halves, a half beside two stacked
  zones, quarters; columns on an ultrawide, rows on a monitor turned on its side. With no
  workspace open, Arrange makes a new one.
- Undo arrange puts every window back where it was, for ten minutes.

### The quick menu

- A thin line at the right edge of the main screen. Rest the pointer on it for Arrange, Undo
  arrange, the switcher, Save layout, Edit zones, snapping on or off, and Open Mosaïk. It never
  takes the keyboard from the app you are typing in, steps aside for a game or a video in full
  screen, and stays where you drag it along the edge. Settings: show it, and on which edge.

### Zone editor

- Four more presets: a half beside two stacked zones, its mirror, two rows and three rows.

### Under the hood

- `mosaikctl state` lists the quick menu, where it is and its rows. `mosaikctl arrange <layout>`
  uses a layout, and `mosaikctl preview [<layout>]` says what Arrange would do without doing it.
  The protocol is version 4; settings are format 4, and settings from 1.0 move to it on their
  own.

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
