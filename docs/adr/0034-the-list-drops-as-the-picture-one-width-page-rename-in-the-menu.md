# 0034. A workspace's list takes drops as its picture does, its page is one width, and its menu renames it

- Status: accepted
- Date: 2026-10-16
- Amends: ADR 0031 (a workspace's apps listed by zone, the menu), 0033 (apps dragged in their
  list)

## Context

The owner, on 2026-10-16, with three pictures of 1.0.2:

- Of a workspace's list of apps, after an app dropped into another zone traded with the app there
  ("Claude and MSYS2 terminal traded zones."): "when drag drop from zone here it permutes instead
  of adding, should be same as in anim permute when specifically dropped on another app otherwise
  just ad". A drop anywhere on a row was onto that app, and a zone's rows are most of the zone.
- Of a workspace's page, its picture narrower than its sections and on the left: "the view in
  image two is disgraceful we should be more appealing symetrical or something it feels off
  currently".
- Of a workspace's ⋯ menu on Home (Open, Edit, Edit zones): "in dropdown we should have rename,
  and not sure we should keep edit zones it feels like it should be on workspace edit screen".

## Decision

**In the list, only an app's icon and name are the app**, as the picture's title bars are: an
app dropped there trades places with it (zones, or in one zone their places front to back). The
rest of a zone, its rows beside their names included, is the zone, which the app joins, behind
its apps. Dragged over a name, the name is outlined; over the rest of a zone, the zone is lit.
`editing::move_app` now puts an app moved into a zone behind the apps there (it kept its place
front to back before, so an app could land in front of them), as a window dropped onto a zone of
Home's picture joins it (ADR 0033); two apps trading zones still take each other's places. The
hint to drag is the grip's tooltip: as the row's, it was each row's name for screen readers.

**A workspace's page is one width, the picture's, in the middle**: the back link, the title and
Open, the picture, Apps, Shortcut and Zones line up on both edges, as Home's title, picture and
layouts do (`pictureWidth`: as wide as there is room for, the picture at most 38% of the window's
height, the page at most 46rem). Its counts ("4 apps · 3 zones") are spaced as the rest of their
line.

**The ⋯ menu of a workspace on Home**: Open (Open again for the one on screen), Replace with
what's on screen (the one on screen), Rename, Edit. Rename turns the row into its name, chosen to
be typed over: Enter saves it, Escape leaves it as it was and gives the focus back to the menu's
button, a name Mosaïk refuses (taken, empty) is said in place. Edit zones is on the workspace's
page (its Zones section), where the zones are, and in the side menu for the open workspace.

## Consequences

- Core test, failing first: an app moved into a zone goes behind its apps, with and without a
  tab group, and two apps trading zones keep each other's places (core 307).
- Interface tests, failing first: the menu's items for the workspace on screen and another one;
  a rename in the row (Escape, a name taken, a free name) (interface 145).
- Playwright, failing first: an app dropped in a zone beside another app's name joins it behind
  it, onto the name trades; the page's picture, sections and Open line up, in the middle, at two
  window sizes; a workspace renamed in its row (with axe). The drag helper brings what it drags
  into view first, as a person scrolls to it.
- The UI tour reaches the zone editor through the workspace's page, and pictures the rename in
  the row.
