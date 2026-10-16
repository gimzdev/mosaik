# 0024. The window rebuilt, dark by default, apps by their icons, no alerts

- Status: accepted
- Date: 2026-10-13
- Amends: ADR 0015 (the quick menu, now the side menu), ADR 0016 (the home page), ADR 0022 (sets
  of apps), ADR 0023 (a simpler app)

## Context

The owner, on the first day of 1.0 in the Store: an alert about the settings file, no presets,
no smart layout and no side bar in sight, wording "like to a child but not even clear", and "the
dark theme default was much better". Then: "break everything and redo everything", "production
ready".

What the owner saw was 1.0 from the Store (it reads settings format 1) on a PC where a 1.1 build
had written format 5: 1.0 said so in a dialog. But the complaint stands for 1.1 too: the window
was a page of sentences, the layouts were words in a list, apps were names, and every problem
interrupted with a dialog or a banner.

## Decision

- **The window is rebuilt** (every component, test and style is new; the engine, its protocol
  and its tests are kept and extended):
  - A bar with Mosaïk's mark, **Home** and **Settings**, Mosaïk's notes (a bell with a count, only
    when there is one) and the switcher's shortcut, which opens the switcher.
  - **Home** has two parts side by side. **Your screen**: the main screen as the chosen layout
    would leave it, each window a tile of its zone's colour, a thin line of desktop apart, with
    its app's icon, its name and its zone's tag in its title bar, over a calm wallpaper (an
    evening sky over soft hills: no grid). Under it, **Smart** (Arrange's own layout for what the
    windows are for, "Smart layout for coding") and the **ten layouts**, drawn; the one chosen is
    named, the picture shows it before anything moves, and a line says what it would minimise.
    **Arrange** does it; Undo puts everything back. Choosing, then arranging, replaces pointing to
    preview and clicking to arrange at once: the picture shows what will happen, never a pile of
    overlapping windows. A window of the picture dragged onto another trades places with it (the
    owner: "we should be able to drag drop apps to permute them"), or takes an empty zone of the
    layout; a click or Enter picks one and a second puts it there, Escape lets go. Each window
    keeps its zone's name and colour as it moves, and Arrange puts them there (protocol 8: the
    moves go with `preview_arrange` and `arrange`; `mosaik_core::arrange::plan_moved` makes room
    again for a window that needs more). Reset puts the picture back. **Workspaces**: a column of rows, each with its layout in small, its
    apps' icons and its shortcut; one click opens one. The suggested sets of apps (ADR 0022) follow
    under **Suggested**. **Save layout** keeps the screen as a new workspace, under a name that
    is free.
  - A workspace's **page** (from its card's ⋯ menu) has its apps and their zones, its name, its
    shortcut, its zones and Delete. The zone editor opens from there, or from the card's menu,
    and its presets are the same ten layouts ("Start from a layout").
  - **Settings**: General (the switcher's shortcut, starting with Windows, the theme), Side menu,
    Windows (snapping, zone tags, coloured borders), Advanced, closed until asked, then About.
- **Dark by default**: settings format 6. 1.0.0 followed Windows' theme by default, so a file of
  format 5 or older with that theme becomes dark; a theme picked (light, or dark) stays. From 6,
  "Like Windows" is a choice, and is kept.
- **No tour**: the first time, Mosaïk sorts the windows open into workspaces and, installed from
  the Store, starts with Windows (a build run from a folder is left as it is: its Run entry would
  name a folder that may go away). Home shows what Arrange will do from the start.
- **Apps by their icons**: the engine names each app's icon (`app:<AppUserModelID>` or
  `exe:<path>`, protocol 7) and reads it on a thread of its own, once per app, as a 48 px PNG
  (Windows' shell image factory, `mosaik_win::icons`); the window asks for the icons it shows
  in one batch (`app_icons`). A desktop app's window is known by its program (the id its windows
  give, such as Chrome's "Chrome", may not be the Start menu's), a packaged app by its id, and an
  app a suggested set starts through its Start menu entry by that entry. An app without one
  shows no picture: never a made-up one.
- **No alerts**: what Mosaïk has to say while it works (another arranger, a window it cannot
  move, a shortcut taken) shows in a **toast**, a small native panel by the corner of the screen
  the pointer is on, that fades by itself and never takes the keyboard; it stays among the
  window's notes. Mosaïk's window never opens by itself. A problem with something the user just
  did is said next to it, in its words. Settings saved by a newer Mosaïk are read as well as
  they can be and shown, unchangeable, with one line saying so: never a dialog, and nothing is
  saved over them.
- **The side menu** (the quick menu of ADR 0015, named as the owner calls it): Mosaïk on its
  first line (a click opens the window; the workspace open is marked in the list below rather
  than named twice), Arrange, a strip of five layouts drawn in small (the one pointed at is
  named above it), the workspaces with their layout in small and the one open marked, Save
  layout, Edit zones and Snap while dragging. It, the switcher and the toasts share the window's
  dark colours.
- **The site** goes back to the home page of its first version, which the owner preferred: "Every
  project gets its own workspace: your apps and windows arranged and ready in one click", desktops
  of two to four windows (Mosaïk lays out three to a view at most; a site shows the idea), each
  window in a coloured frame with its zone's tag above it (both are settings of the app) and a
  thin line of desktop between windows. The bar that desktop once had along its top is left out:
  Mosaïk draws no such bar. The section "One desktop, many kinds of work." ("Who it's for", four
  desktops in small) is gone for good (the owner, 2026-10-14: "it looks stacked and is
  redundant with first section ... erase that"): the hero shows the same four workspaces.
- **Version 1.0.1**: first 2.0.0, since the settings format changed in a way 1.0.0 cannot read
  and the window is new; then the owner, the same day: "rename everything to be v1 not v2 ...
  we're on v1.0 all the time". 1.0.1 is the lowest version the Store takes after its 1.0.0.

## Consequences

- 1.0.0 cannot read format 6: a PC that goes back to it shows 1.0.0's own message once. 1.0.1
  reads every older format.
- Icons cost one read per app per run, off the engine's thread; the cache is in memory only.
- The tests of the window were rewritten with it (component tests with `@testing-library/jest-dom`
  for readable DOM assertions, a test-only dependency, and the browser smoke tests); the UI tour
  of `mosaik-probe` walks the new pages in the three themes.
- Words were cut everywhere: a control says what it does, a summary says what the picture shows,
  a problem says what happened and what to do.

## Amended by ADR 0028 (2026-10-15)

A click on a workspace shows how it opens; Open opens it. Zones are numbered by place.

## Amended by ADR 0029 (2026-10-15)

Larger text and controls, Automatic and five named layouts, Back from a preview, results under the
picture only, "On screen" for the workspace open, "Save as workspace", and Settings in the order
people use them.

## Amended by ADR 0031 (2026-10-15)

Eight layouts besides Automatic (no Top + bottom or Three rows), all in view; the picture draws
zones, with the windows' title bars in them, and its lines resize them; Arrange windows.
