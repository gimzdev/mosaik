# 0014. Arrange: every window in the zone that suits it, with undo

- Status: accepted
- Date: 2026-10-12

## Context

The owner asked for "a smart organisation button, like sorting, auto, very smart". Until now
windows went into zones only by hand (a drop, a save, a workspace opened), and a workspace had
to be saved before its zones existed.

## Decision

- **One command, Arrange**: in the tray menu, the quick menu (ADR 0015), the home page
  (ADR 0016) and `mosaikctl arrange`. Every window on screen that Mosaïk manages goes into the zone that
  suits it. The open workspace takes them in; with none open, a new one is made ("Workspace 2"
  and so on). It moves windows the way opening a workspace does, all at once, and saves the
  workspace.
- **A pure planner** (`mosaik_core::arrange::plan`, tested on its own) decides; the engine only
  gathers the windows and applies the plan.
- **Zones where there are none.** A monitor with windows and no zones of the open workspace gets
  a preset (`Preset::for_windows`) by how many windows it has and its shape: one window, the
  whole monitor; two, halves (rows on a monitor turned on its side); three, a half beside two
  stacked zones (three columns on an ultrawide, three rows on a monitor on its side); four or
  more, quarters (thirds on an ultrawide, three rows on its side). Never more than four zones on
  a monitor: more windows become tabs. Monitors that have zones keep them.
- **Kinds of apps**, from their program, or their package for Store apps: code, terminal,
  browser, chat, mail, notes, documents, design, media, files, other (`Kind::of`). A zone named
  for a kind ("Browser", "Terminal") or after an app ("Notepad++") draws those windows.
- **Each window and zone pair gets a score:** +100 when the zone is named after the window's
  app; +60 when it is named for its kind, −80 for another kind (a terminal does not go in
  "Notes"); +45 when the window is in it already; up to +30 for how much of the zone it covers;
  up to +25 for the window's importance matching the zone's size (importance: 65% its kind,
  code and design first, media last; 35% how recently it was used), so the windows that matter
  most get the largest zones; a window of a fixed size instead scores up to +35 for how close
  its size is to the zone's; −60 for a zone on another monitor. Pairs are taken best first,
  each window and zone once, down to a score of 0; ties go to the window used last, then to the
  zone listed first, so the same screen always arranges the same way. New zones go to the
  windows on their monitor before any is left empty.
- **Windows left over** take an empty zone, unless a zone with a window in it is their own
  (named after their app or for their kind: a score of 50 or more), where they join as a tab.
  With every zone taken, they join, as a tab, the window in the zone that suits them best. A
  tab group moves as one, as its visible tab.
- **New zones are named** after the window that gets them: its kind ("Browser"), else its app;
  "Browser 2" when the name is taken. Their colours follow the palette's order.
- **Undo**, for ten minutes, until a workspace is opened, saved or edited: the workspace goes
  back to what it was (or away, if Arrange made it), the workspace open before is open again,
  every window Arrange moved goes back where it was (maximised again if it was), and the
  windows it made background tabs show again as they were.

## Consequences

- Arranging again moves nothing: windows already in zones score +45 to stay.
- Kinds come from a list of programs in `mosaik-core`; an app it does not know is "other" and
  is placed by its size, place and how recently it was used. Zone names are the user's way to
  steer it: name a zone "Terminal" and terminals go there.
- Arrange adds the windows it places to the workspace: opening it later brings them back there.
- Measured with `mosaik-probe smart` (`docs/STATUS.md`).
