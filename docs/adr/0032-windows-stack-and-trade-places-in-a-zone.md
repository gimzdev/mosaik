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
rest: the plan's last step (`arrange::stack_the_rest`) puts each window left over in a zone it
fits, behind the window there, as several windows of one zone already were in a workspace: with
windows of its kind first (two browsers together), else where there are fewest, else the larger
zone. Only a window that fits no zone, even stacked, still waits in the taskbar (ADR 0021: the
zones make room first). The picture lists a zone's windows front to back, as before.

**Two windows of one zone trade places in it.** Dropped onto another window of its zone (or
picked, then Enter on that one), a window trades places with it:
- On Home's picture of the screen, front to back: the picture lists them so at once, and Move
  windows puts the one that was behind in front (protocol 11: `onto` on a move, the window of the
  same zone; `arrange::traded_order`). The windows are stacked each just below the one before
  (`put_below`, on a thread of its own), so none comes up over Mosaïk's window. A trade waiting
  is a change to make, on a screen already arranged too ("Undo changes", not "already like
  this"), and Move windows makes it once: each swaps the order on screen then, so Home drops the
  trades once done (sent again, they would trade the windows back), keeping the windows put in
  other zones. The plan stays as the screen is: a window stacked stays in the zone it fills.
- On a workspace's picture (Home's preview of it, its page), saved at once: two apps of one zone
  trade their places in it (`move_app` with `swap_with`, `editing::trade_in_zone`). Their order
  is the workspace's own, front to back, which opening it follows; the picture and its page list
  them so (`Layout::ranks`: a tab group where its front-most tab is). Two tabs of one group trade
  places in its strip, the tab showing staying the one showing; a tab and a window trade the
  tab's place in its group, showing if it was, for the window's. In the open workspace the
  windows are stacked in the new order at once, and the strip follows.
- Two pieces of one zone that trade fly to their places, as a piece changing zone does; with the
  keyboard, the focus goes with the piece picked.

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
  their kind, else fewest), and a window that fits no zone still waits in the taskbar; two apps
  of one zone trade places (two tabs of a group, two windows, a tab and a window, tabs of two
  groups, and back); the apps' ranks front to back; trades in order. Interface tests: a window
  stacked behind another brought to the front on Home (at once, sent with `onto`, the focus going
  with it); two tabs and two windows of one zone traded on a workspace's page. Playwright: Slack
  stacked behind Visual Studio Code in Thirds, dragged onto it, comes first, and Move windows
  says "Moved 4 windows into Thirds."; Claude dragged onto Chrome in Backend's strip.
- The `smart` probe: the arranged screen's windows all in zones (none minimised) for a new fifth
  window and for Halves; two windows of a half traded on the picture, then arranged (the one
  behind now in front, both in the half, Undo); two apps of the open workspace's zone traded
  (listed the other way round, the picture too, the first one's window in front, both filling
  the zone, all put back). The windows put elsewhere are taken from two zones, now that one zone
  can hold two.
