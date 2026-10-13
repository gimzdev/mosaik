# 0015. The quick menu: a line at the screen's edge

- Status: accepted
- Date: 2026-10-12

## Context

The owner asked for "a side line opening a menu when hovering, mid right or top right by
default, just a line at the end of the screen which expands on hover, opening the auto sort
button and other useful options". Until now the pointer reached Mosaïk through the tray icon,
in the corner farthest from most windows.

## Decision

- **A native overlay**, like the frames and tab strips (ADR 0002): one layered window
  (`mosaik_overlay::edge`), drawn with Direct2D, on the engine thread. At rest it is a line 4 px
  wide and 56 px long (at 100%) on the right edge of the main monitor's work area, 30% of the
  way down; it widens when the pointer is on it. The strip that takes the pointer is 8 px wide
  where the pointer stops at the screen's edge anyway, 12 px where it goes on (another monitor,
  or the taskbar, past that edge). Every pixel of it is faintly opaque, so all of it takes the
  pointer.
- **It opens when the pointer rests on it** for 300 ms (Windows' own hover tracking), or on a
  click, and closes 400 ms after the pointer leaves, or when a row is picked. Passing by on the
  way to a scroll bar does not open it; slipping off for a moment does not close it.
- **It never takes the keyboard**: the window does not activate (`WS_EX_NOACTIVATE`, and
  `MA_NOACTIVATE` on a click), so whatever the user was typing in keeps the focus.
- **Rows**: Arrange windows (the main one, ADR 0014); Undo arrange, while Arrange can be undone;
  Switch workspace, with its shortcut; Save layout; Edit zones, with a workspace open (it opens
  the zone editor); Snap while dragging, On or Off; Open Mosaïk (its window, on the home page,
  ADR 0016). Its header names the open workspace.
- **Dragging the handle** along its edge moves it; its place is kept in the settings, in
  thousandths of the edge's height, so it stays within a pixel of where it was let go.
  Settings: "Show the quick menu" (on by default) and "Where" (right or left edge).
- **Out of the way of full screen**: while a game or a video fills the main monitor in front, the
  handle hides; it comes back when that window leaves full screen, is minimised or closed, or
  something else comes to the front. It is off in safe mode.
- **On top when drawn**: it is put back above windows kept on top (a video player, Task
  Manager) whenever it is drawn, and the menu opens above them.
- **Not the only way**: every row is also in the tray menu, the switcher or the settings window,
  which the keyboard and screen readers reach. The menu is a shortcut for the pointer.
- `mosaikctl state` reports where the handle or the open menu is and its rows, so tests can
  point at them by their numbers.

## Consequences

- A strip 8 px wide (12 px where the pointer goes on past the edge) and 76 px tall covers that
  much of what is under it, on top: a window's scroll bar at that height loses that much. The
  handle can be dragged elsewhere along the edge, put on the left edge, or turned off.
- At rest it costs nothing: it is drawn again only when the pointer comes or goes, the settings,
  the theme or the monitors change.
- Measured with `mosaik-probe smart` (`docs/STATUS.md`).
