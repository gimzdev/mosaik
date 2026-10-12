# Changelog

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
  above it, inside its ring, so the two read as one window.

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
