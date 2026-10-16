# 0028. Zones are numbered by place; a workspace is shown before it opens, and opening it clears its screens

- Status: accepted
- Date: 2026-10-15
- Amends: ADR 0014 and 0019 (zones named after their windows), 0016 (Home), 0018 (a
  workspace's apps), 0024 (the window rebuilt)

## Context

The owner, on 2026-10-15, on a workspace's page: "the names in the dropdown like assistant code
zone etc are absolute bullshit they should be 1 2 3 4 5 6 or color names"; "why is there options
in dropdown for other apps in an app row", "why is 'zone' an option amongst apps": zones were
named after the window Arrange put in them ("Code", "Browser 2", "Zone"), so the list of zones
read as a list of apps. Then: "as its redundant with the little dot we should merge the two things
and the 'remove' should be a cross", "show all dots and the selected one is highlighted", "for all
apps"; and the page should "show visually the zones same as homepage for that workspace
specifically with arrangements aswell".

On Home, a click on a workspace opened it at once. The owner: "we shouldnt open the workspace when
clicking on it it should show it in the anim as preview and show an open button". Opening
"Coding" left a game launcher and the Riot client on top of it ("here's what happens when i click
on code wtf"; his log: moved=3 minimized=0): only the windows of the workspace left were
minimised.

## Decision

- **A zone's name is its number in reading order**: monitor by monitor, row by row from the top
  (a zone within 2% of another's top is in its row), then from the left
  (`Layout::number_zones`). Arrange, saving a layout, a set's workspace, the zone editor and
  zones making way for a layout all number their zones; the editor has no name to type, and a
  name a client sends is not kept. A workspace file with names from before is renumbered once on
  load, with one note; its format stays, so an older Mosaïk still reads it. A window moved on
  Home's picture keeps its colour; the zone keeps its number.
- **A workspace's page** shows the picture of the main screen as opening it would leave it, and
  lists its apps one row each: the app's icon and name, a dot for every zone (its number on its
  colour) with the app's own zone lit, and × to take the app out. A click on another dot moves the
  app there. No list, and no "A tab" line.
- **Home shows a workspace before it opens**: a click on a workspace picks it, and the picture
  shows what opening it would do, from the plan opening it follows (`preview_workspace`, protocol
  9): its apps glide from where they are to their zones, an app it would start is drawn dashed
  where it will open, a tab group shows its tabs above the tab that shows, and the windows it
  would minimise are named below. Open takes Arrange's place; once it opens, or when a layout is
  chosen or the workspace clicked again, Home goes back to Arrange. A workspace's ⋯ menu opens it
  at once.
- **Opening a workspace minimises the other windows on the monitors it is on**: every window on
  screen there that it does not use (not a window in full screen), and the workspace left's
  windows wherever they are. Windows on its other monitors stay. The setting "Minimize other
  windows when a workspace opens" turns it off.

## Consequences

- Tags on windows show a number; the colour stays the zone's own.
- The probe checks that a saved workspace's zones are numbered 1 to N, that its windows land
  where the preview put them, that a window always on top that is not the workspace's is
  minimised when it opens, and that zones split in the editor are numbered by place.

## Amended by ADR 0029 (2026-10-15)

A second click on a picked workspace keeps it picked; Back (or Esc) returns to the screen as it
is, and a double-click opens the workspace.

## Amended by ADR 0031 (2026-10-15)

The picture draws each zone once, empty ones too, with its windows' title bars in it rather than
each window where it lands (two of one zone were drawn over each other), and a workspace's apps
and zones are changed on it, saved at once. A workspace's page lists its apps zone by zone, each
with Move and ×, instead of a row of dots for each.
