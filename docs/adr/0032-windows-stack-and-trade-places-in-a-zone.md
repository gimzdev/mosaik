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
down back under the nearest window above it that was shown and alike, Mosaïk's frames, tags and
strips aside, or on top of the others when none was.

**As the screen is.** Zones that Move windows made with the layout asked for now (the same layout
picked, or the layout of the task both times) are that layout again while every window there is
still in the zone it was put in, filling the part of it a window takes (below a tab group's strip
or a tag's band, inside the gaps: `Unit::inset`; a window that keeps a size there, its own or a
least size larger than that part, where the mover puts it, `place::fit_size`), or the zone itself,
and, for the layout of a task, no zone it put a window in is empty (step 0 of the plan,
`KeptZone::arranged`): each window stays in its zone, the one in front its own, whatever task
the windows, stacked anew, say now, and the zones keep the room they made for the user's moves.
A window opened since, closed or taken out of its zone lays the screen out anew; then too,
windows already in a zone of the new layout stay as they are on screen: of the windows filling a
zone, the one in front is its own window (step 3). So arranging again moves nothing, and a window
brought to the front of its zone stays there. Once Move windows has made the user's moves and
stretched zones, Home lets them go: the screen holds them. The zones as stretched on the picture
are the plan's own (`Stretched`), so that the windows are fitted to them; on an arranged screen,
they stretch its zones.

**Colours go with the windows.** Each zone takes the colour its own window wears on screen, else
one that a window stacked in it wears, else the colour it has on screen itself (an empty zone of
an arranged screen, its colour gone where the user's moves took it), else the plan's; each colour
once, the zones kept on another monitor counted (`arrange::zone_colors`). A trade takes the
colours along with the windows, and once Move windows has made it, the picture and the screen
agree.

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
- Core tests for the third review, each failing first: Outlook with Steam brought to the front
  of its zone stays as arranged (the next plan laid it out in halves); a move that made room
  stays made; a window opened since lays the screen out anew, one closed too for a task's layout
  (a layout picked keeps its zones); on 6000 random screens with moves, the next plan finds every
  window where it is, and a line dragged on the arranged screen's picture stretches its zones
  within the plan's rules; zones take the colours their windows wear, after a trade and laid out
  anew, each colour once; a window that keeps its size stays in front once arranged; zones
  stretched in a task's layout make room for their windows; a task's layout leaves no zone
  empty; a drop onto a zone whose window left trades with the one now in front. Interface: Home
  lets the zones stretched go once Move windows has made them. Playwright: two windows traded
  across zones keep their colours after Move windows, and Move windows again moves nothing.
- The `smart` probe: the arranged screen's windows all in zones (none minimised) for a new fifth
  window, for Halves and from the side menu; Halves puts the zone's own window in front of the
  one stacked behind it; two windows of a half traded on the picture, then arranged (the one
  behind now in front, both in the half, Undo); two apps of the open workspace's zone traded
  (listed the other way round, the picture too, the first one's window in front, both filling
  the zone, all put back). Each check of the order first puts the windows the other way round.
  The windows put elsewhere are taken from two zones, now that one zone can hold two. After
  Halves, a trade in a half, a window put elsewhere and Halves stretched, the picture read again
  finds each of the probe's windows where it is, first in its zone where it was, in its colour
  (`kept_as_arranged`); Undo puts the windows back in their order front to back too.

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

## Third review (2026-10-16)

A third agent reviewed 5b6280a and found, each now caught by a test written first: a move that
made the zones make room was undone by the next plan, which made the layout's zones anew without
the move (Home had let it go) and found the moved window in none of them; a window that keeps
its size was never found in its zone, where Move windows puts it, so a window stacked behind it
took its place; zones stretched in a task's layout were not fitted to their windows; a task's
layout could leave a zone empty when the window filling a zone was not the one it had given it;
a window dropped onto a zone whose own window had just left took it as empty; Undo put windows
back under Mosaïk's own frames and tags, which had moved since, or under a window always on top,
and so left them in front; the probe's check of a second picture passed for windows it would
minimise, and counted the owner's windows; the stand-in kept no window filling its zone there.
Measured on random screens, a trade in a zone of a task's layout made the next plan lay the
screen out anew one time in ten: the windows, stacked anew by Move windows, said another task.
Hence the screen as arranged (step 0) and colours that go with the windows, above. On 80000
random screens of 2 to 9 windows (one in five of a minimum size or one they keep, half with one
or two moves), a second Move windows moves no window, where it moved one on 220 to 849 of 20000
after the second review.

## Fourth review (2026-10-16)

A fourth agent reviewed ef9d140 and found, each now caught by a test written first: a window
below a band (a tab group's strip, or every window's tag when tags go above the windows) covers
less of its zone, and with a band over a tenth of the zone's height the next plan did not find it
there (on random screens with one tab group at 125%, 250 of 6000 moved a window; with tags in a
band, 554); in a layout picked, the window filling a zone went to another zone when the window
the layout had given that zone fit no other, which only a task's layout must avoid (47 of 20000
screens, a regression of the third review's fix); the empty zones of an arranged screen took the
plan's colours, not theirs; a colour could repeat that of a zone kept on another monitor; the
stand-in measured a window against its zone alone, not against the larger of the two, and kept
the arranged screen after Save or a set opened. Now a window is in a zone when it fills the part
of it Move windows puts it in (`Unit::inset`, from `FrameStyle::window_insets`, with the tab
style for a group's shown tab); the third review's guard holds for a task's layout only; a zone
that is the screen's again keeps its colour there (`NewZone::kept`); the colours of the zones kept
are not given again. On 6000 random screens with bands, gaps and tab groups at 100% to 150%, the
next plan finds every window where it is.

## Fifth review (2026-10-16)

A fifth agent reviewed 5d4705f and found, each now caught by a test written first: a window
that goes no smaller than more than its part of a zone is put there by Move windows at the size
it keeps, over the band (`place::fit_size`), so it did not fill its part and the next plan did not
find it there; in a layout picked, the window the layout gave a zone that another window filled
was put where that window had been even where it did not fit, until the last step took it out,
so the picture showed that zone empty and a window dropped onto it traded places with one not
shown (40 of 4497 random drops onto a zone shown empty moved another window); a window filling a
zone exactly, as Windows' snapping or another tool leaves it, did not fill the part below a band
or inside wide gaps and was stacked elsewhere (157 of 20000 random screens); the tabs of a group
that cannot be hidden were measured without their strip's band. Now a window is in a zone when it
fills its part, sits where the mover puts it at the size it keeps, or fills the zone itself; in
a layout picked, that window is stacked where it fits and the zone left stays empty; every tab of
a group is measured below its strip.
