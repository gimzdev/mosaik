# 0031. The picture shows each zone once and is where windows and zones change

- Status: accepted
- Date: 2026-10-15
- Amends: ADR 0016 and 0024 (Home), 0028 (a workspace's page and preview), 0029 (what Home says,
  the layouts, the welcome)

## Context

The owner, on 2026-10-15, with three pictures of Mosaïk 1.0.2: the first-run line on Home ("this
shit has to go"); Home's picture of "Coding", where two windows of one zone were drawn one over
the other, title bar over title bar, and the empty zone had neither colour nor number ("look at
the anim wtf is happening", "when a zone is empty we dont have either color or number"); and a
workspace's apps, each with a row of zone dots ("the options look like absolute garbage", "maybe
we should use another way i let you judge whats best"). Also: no More layouts list ("we dont want
dropdown menu"), and no layouts of rows only ("noone ever uses that shit and if they want they can
custom"), "9 left fitting in 2 lines of options"; Arrange to be named for what it does, with an
icon other than Automatic's spark ("apply is not clear enough its like save a little bit", it
should say it takes over the screen); and on the picture, Home's and a workspace's, windows
dragged and zones stretched ("we should be able to stretch zones", "the anim must have those
options too (drag drop and stretch zones)"). Then, of the bars drawn between zones in the zone
editor: "way too large", "either discreet or not shown".

## Decision

**The picture** (Home's, a workspace's page's)
- Each zone of the main screen is drawn once, a tile of its colour with its number at the top
  right, as Mosaïk tags windows on screen. In it, the windows it takes are listed by their title
  bars, front first, one under the other: a tab group is one bar of its tabs, an app a workspace
  starts is dashed ("starts"). A zone with more windows than room shows "+2 more". An empty zone
  keeps its colour (lighter, dashed) and number and says "Empty". A window in no zone (full
  screen, a place of its own) is not drawn; the words for screen readers say where it goes, and
  which zones are empty ("Zone 3 is empty.").
- Mosaïk says which zone each window goes in, and which app each window and tab is, rather than
  the page working it out from rectangles, and each zone's exact place, which the lines start
  from rather than from rounded pixels (protocol 10: a preview zone's `id` and `area`, a window's
  and a tab's `zone` and `app`).
- A window (or a tab) is dragged by its bar: a ghost follows the pointer and the zone or window
  under it lights up; let go, it flies to its new place. Without a mouse: Enter picks it, the
  other zones become buttons ("Zone 2", "Zone 3, empty"), and Enter on one of them or on another
  window puts it there, the focus going with it; Esc lets go.
- The lines between zones are dragged (or moved with the arrow keys, Home and End once focused)
  on the zone editor's grid of 48ths, never below its smallest zone. At rest a line is only the
  gap between two zones; the pointer over it shows a thin line and a resize arrow, as dragging or
  the keyboard's focus does. The same lines in the zone editor.

**Home's picture of the screen** (before Arrange windows)
- A window dropped onto another, or onto a zone, trades places as before (`moves`). Lines
  dragged resize the zones Arrange will make (`areas` on `preview_arrange` and `arrange`: the
  main monitor's zones in the picture's order, `editing::stretch_zones`, taken when there is one
  area for each, none under the smallest zone and none over another; the zones are then numbered
  by place, so a number follows its zone). The windows keep the zones the plan gave them.
- What was changed stays chosen after Arrange windows: the picture keeps showing the screen as
  it now is ("Your windows are already like this."), rather than the layout's own shape again.
  "Undo changes" shows while the picture differs from the screen and puts the layout's own back;
  another layout, a workspace opened or a set started starts afresh, and areas made for other
  zones (other windows on screen) are let go.
- The main button is **Arrange windows**, as the tray says it, with Mosaïk's four tiles (its
  mark); Automatic keeps the spark. ("Apply" read as saving; "Arrange my screen", tried first,
  the owner: "cringe af", "dont be brosky nor corporate just balanced and pro yet casual".) The
  line under the title says "Automatic layout for coding. Nothing moves yet."
- The layouts are all in view: Automatic as tall as two rows, then Halves, Thirds, Main + side,
  Side + main and Half + stack, Stack + half, Quarters, Full screen, each drawn above its name. Top
  + bottom and Three rows are no longer offered (Home, the zone editor's presets); the engine
  keeps them (`mosaikctl arrange rows`), and a zone splits into rows in the editor.
- No word of welcome: the first run says only which workspaces it made of the windows open.

**A workspace's picture** (Home's preview of it, its page)
- Changes are saved in the workspace at once, as its page's list saves them: an app dropped onto a
  zone goes in it (`move_app`); dropped onto another app, the two trade zones (`swap_with`,
  `editing::swap_apps`), a tab leaving its group; in the open workspace the windows move at once.
  Lines dragged save its zones (`zones`, then `set_zones`), where only the edges moved change, so
  the others keep their exact place rather than one read back from the picture's pixels; changes
  wait for the one before. On Home the line under the picture says so ("Visual Studio Code goes
  in zone 2 of “Coding”."); what Arrange or opening a workspace did can no longer be undone then,
  as after any change to a workspace.
- Its page lists the apps zone by zone, as the picture shows them: each zone's number on its
  colour, a line of its colour down to its last app, a zone left empty saying so with how to fill
  it ("Empty. Drag an app here on the picture, or use Move."). Each app has Move (a menu of the
  other zones, each with its number and colour) and × to take it out; a tab says so.

## Consequences

- Protocol 10. An older client sends none of the new fields and reads replies as before.
- Core tests: a monitor's zones take the areas stretched on the picture, and are refused when
  their count differs, one is too small or two cover each other; two apps trade zones (a tab
  leaving its group). The interface's presets are checked against the engine's, but for the two
  of rows only.
- The `smart` probe arranges Halves stretched to two thirds (the picture's zones and windows
  first, changing nothing, then the windows in those zones, then Undo), and has two apps of the
  open workspace trade zones (the workspace and the windows, the third window staying). The UI
  tour drags a line of Home's picture, then Undo changes, and looks for Arrange windows.
- Sample names read as people name things: the stand-in's workspaces are Website, Backend and
  Portfolio, the tour's Website, and the site's coding project "Client website" (it was "Ship
  the release": "cringe af"). Renamed for the work done in them by ADR 0038.
- The stand-in engine (`mock.ts`) has a workspace like the owner's "Coding" ("Backend":
  two apps and a tab group in one zone, a zone left empty).

## Review (2026-10-16)

A second agent reviewed the first commit (596ba4f) and found four bugs, each now caught by a test
written first: a line drag let go of with Esc was saved later into the workspace shown next, on a
click on its line that moved nothing (a line now ends a drag only when it moved, and only on the
picture it was dragged on); a window picked, then let go of because the picture could not change
(Arrange windows), came back picked, unsaid (let go for good now); a line's limits came from the
picture's rounded pixels while the edges not moved kept their exact place, so a zone resized to
the smallest came out a hair under it and was refused (the preview now sends each zone's exact
`area`); and the focus fell to the page when a tab was put in another zone with the keyboard (it
now waits for the tab's new place). Also: a drag whose window left the picture mid-drag ends on
the release anywhere; a trade of two apps refits the zones they really were in, whatever zone a
picture drawn before a change named (the `smart` probe trades back that way); a resize that
would make a zone too small or put two over each other is not sent.

## Amended by ADR 0032 (2026-10-16)

The main button is **Move windows** ("move windows and open are good yes lets go"); a workspace
keeps Open. Two pieces of one zone dropped onto each other no longer stay as they were: they
trade places in it, front to back or in their tab strip, on Home and on a workspace's picture.
Home's title row, picture and layouts are one width. The areas stretched on Home's picture are
the plan's own zones (`arrange::Stretched`, checked by `editing::areas_fit`), so the windows are
fitted to them; `editing::stretch_zones` is gone.
