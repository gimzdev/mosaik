# 0032. Windows a layout has no zone for stack in its zones, and two of one zone trade places

- Status: accepted
- Date: 2026-10-16
- Amends: ADR 0014, 0017, 0019 and 0021 (windows left over), 0015 (the side menu's first row),
  0016 and 0031 (Home's main button, the picture), 0030 (what Arrange says)

## Context

The owner, on 2026-10-16, with a picture of a workspace like his own "Coding" (MSYS2, then
Chrome and Claude as tabs, in one zone): "we need to be able to move placement of apps in a zone
like in the screenshot i should be able to permute chrome with claude"; "when we have less zones
than apps it just makes them disappear they should stack"; "the alignment of anim and the
buttons at bottom are weird cause buttons go further it doesnt feel simetric and appealing".
And of Home's main button, "Arrange windows" ("should probably be "open windows" or something its
still unclear"), after weighing both ("move windows assume theyre opened which theyre sometimes
(most of times) not. and open windows assume they're not which they sometimes are"): "move
windows and open are good yes lets go".

## Decision

**Windows stack in the zones.** A layout with fewer zones than windows no longer minimises the
rest: once each zone has its own window, the windows left over stack in the zones
(`arrange::stack_the_rest`), behind the zone's own window, as several windows of one zone already
were in a workspace: with windows of their kind first (two browsers together), else where there
are fewest, else the larger zone; a window already filling a zone stays in it. Only a window that
fits no zone, even stacked, still waits in the taskbar (ADR 0021: the zones make room first).
The plan says each zone's windows front to back (`Plan::order`: its own window, then the others,
those used last first); the picture lists them so, and Move windows stacks them so, as little as
it takes: a window in front of the one that should come before it is put just below that one
(`put_below`, on a thread of its own), the others stay, so the order the windows were used in
changes no more than that and none comes up over Mosaïk's window. Undo puts each window moved
down back under the window that was above it.

**As the screen is.** Windows already in a zone of the layout stay as they are on screen: of the
windows filling a zone, the one in front is its own window (step 3 of the plan). So arranging
again moves nothing, a window brought to the front of its zone stays there, and once Move windows
has made the user's moves, Home lets them go. The zones as stretched on the picture are the
plan's own (`Stretched`), so that the windows are fitted to them and found filling them.

**A window dropped onto another trades places with exactly it**, wherever the two are: their
zones, and their places in them, own window or stacked; when both are their zones' own windows,
each zone's colour goes with its window. Dropped onto a zone itself, a window trades places with
the zone's own window, or takes the zone left empty. In one zone this brings a window to the
front, or sends it behind (the owner's Chrome and Claude):
- On Home's picture of the screen: the picture shows it at once, and Move windows does it
  (protocol 11: `onto` on a move, the window it was dropped onto). A trade in a zone waiting is
  a change to make, on a screen already arranged too ("Undo changes", not "already like this").
  Each trade is made on the screen as it is then, so once Move windows has made them, Home lets
  the moves go (sent again, a trade would put the windows back): the plan keeps the windows as
  they are on screen.
- On a workspace's picture (Home's preview of it, its page), saved at once (`move_app` with
  `swap_with`): in two zones the two apps trade zones and their places front to back
  (`editing::swap_apps`); in one zone, their places in it (`editing::trade_in_zone`). An app's
  place is the workspace's own order, which opening it follows; the picture and its page list
  them so (`Layout::ranks`: a tab group where its front-most tab is). Two tabs of one group trade
  places in its strip, the tab showing staying the one showing; a tab and a window trade the
  tab's place in its group, showing if it was, for the window's. In the open workspace the
  windows are stacked in the new order at once, and the strip follows.
- Two pieces that trade in one zone fly to their places, as a piece changing zone does; with the
  keyboard, the focus goes with the piece picked.
- Windows are stacked only among windows alike, always on top or not: under one that is, a
  window would become so; one that is would stop being so under one that is not.
- After the user's moves, the zones make room for the windows stacked behind too, when there is
  room for all; a window stacked in a zone it no longer fits is stacked again where it fits. A
  zone holding stacked windows only has the one in front for its own, as the picture shows it.

**Move windows.** Home's main button, the tray's item and the side menu's first row say **Move
windows**, with Mosaïk's tiles; Mosaïk then says "Moved 4 windows into Thirds." (or "Moved 4
windows."). A workspace keeps **Open**. Neither word assumes too much: Move windows moves the
windows that are open, Open opens a workspace, starting what is missing.

**One width.** Home's title row (the title and the main button), the picture, the layouts under
it and the line under them share the picture's width, so the button ends where the picture
does (`pictureWidth`).

## Consequences

- Protocol 11. An older client sends no `onto`, and reads replies as before.
- Core tests, each failing first: a layout with fewer zones than windows stacks the rest (with
  their kind, else fewest), and a window that fits no zone still waits in the taskbar; each
  zone's own window first in the plan's order (job 0105's screen); a window dropped onto another
  trades places with exactly it (in two zones, in one, onto a zone, onto an empty zone); two apps
  of one zone trade places (two tabs of a group, two windows, a tab and a window, tabs of two
  groups, and back), two of two zones their places too; the apps' ranks front to back.
  Interface tests: a window stacked behind another brought to the front on Home (at once, sent
  with `onto`, the focus going with it), made once by Move windows, a move into another zone kept;
  two tabs and two windows of one zone traded on a workspace's page; the picture before Mosaïk
  says, for each kind of drop. Playwright: Slack stacked behind Visual Studio Code in Thirds,
  dragged onto it, comes first, and stays so after Move windows; Slack dragged onto Chrome takes
  its zone, Chrome its place; Claude dragged onto Chrome in Backend's strip.
- The `smart` probe: the arranged screen's windows all in zones (none minimised) for a new fifth
  window, for Halves and from the side menu; Halves puts the zone's own window in front of the
  one stacked behind it; two windows of a half traded on the picture, then arranged (the one
  behind now in front, both in the half, Undo); two apps of the open workspace's zone traded
  (listed the other way round, the picture too, the first one's window in front, both filling
  the zone, all put back). Each check of the order first puts the windows the other way round.
  The windows put elsewhere are taken from two zones, now that one zone can hold two.

## Review (2026-10-16)

A second agent reviewed the first commit (598f728) and found, each now caught by a test written
first where it is code: a window stacked in a zone, used more recently than the zone's own
window, stayed in front of it after Move windows (Arrange stacked only zones traded in; now
every zone with two windows, own window first); a window dropped onto a window of another zone
traded with that zone's own window whatever window it was dropped on, and a stacked window
dropped there was shown traded, then put behind (now exactly the two trade); the probe's windows
are always on top, so nothing was stacked among them (windows alike are stacked, and the probe
puts the window to bring forward behind first, so its check cannot pass by chance); a window
always on top would have lost it under one that is not; Undo left the new stacking; two tabs of
one group trading places restacked their whole zone. Also: Home kept a trade made by Move
windows and sent it again, trading the windows back.

## Second review (2026-10-16)

A second agent reviewed 414b014 and found, each now caught by a test written first: stacking
every zone pulled each stacked window up to just below its zone's own window, and the next plan
read that as which windows were used last and moved windows; the plan picked a zone's own window
by what the task needs and how recently it was used, so a window brought to the front went back
behind at the next Move windows; after a move, the zones made room for their own windows only,
and a window stacked behind could be left in a zone narrower than it goes; a zone could hold a
stacked window and no own window, which the picture took for its own; the moves Home kept after
Move windows could trade windows back; a stack broke at a window always on top; Undo did not put
windows back among the others. Measured on 20000 random screens of 2 to 9 windows, a second Move
windows moves a window on 13 (no window with a minimum size) and 156 (one in five with one), where
the plan before ADR 0032 moved one on 204 and 357: the remaining ones lay out the screen anew for
a task the windows now say otherwise, or make room in other zones for a window that needs more.
