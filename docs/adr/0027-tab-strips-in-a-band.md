# 0027. A tab group's strip is in a band above it, never in a title bar

- Status: accepted
- Date: 2026-10-15
- Amends: ADR 0011 (strips in title bars), 0017 (strips always in title bars)

## Context

ADR 0017 put every tab group's strip in its visible tab's title bar, at the right end, so tabs
sat in the same place on every window and no window moved for its strip. Where the title bar was
crowded (a browser's own tabs, an editor's menus), the strip had the least room it could do with:
the zone's tag, the visible tab's chip cut short and "+1" for the others. The owner, on
2026-10-15: "the tab thing is still fucked up also when switching a multi tab window from a zone
to another the tab list disappears and we cannot drag only 1 tab it drags the whole window". With
no chip for a tab, there was nothing to pull it out by: a press on the tag drags the whole group.
After a group was dropped in another zone, its strip waited for the title bar to be read again
at the new size, and the app, raised by the drop, could cover it meanwhile.

## Decision

- **A group sits below a band with its strip, always**: with tags in title bars as with tags
  above windows, as when tags are hidden (`FrameStyle::for_tabs`). Every tab has its own chip at
  its own width, as many as the band holds, so any one can be dragged out; the window keeps
  the rest of its zone. Windows alone are unchanged: their tag stays in their title bar, cut
  short or to the zone's initial when room is short.
- **The strip's place is known without reading the title bar**: the band of the zone holding
  the group, right above the visible tab, at once after a switch, a drop or a click on a tab.
  After a group is dropped, Mosaïk puts the strip back above the window twice (after 120 ms,
  then 400 ms later), once the app has raised its window.
- **What only served strips in title bars goes**: moving a window into a band when its title
  bar had no room for its group's strip (with its two readings a quarter of a second apart and
  its three seconds after the user's action), and the strip's place in a crowded title bar.
- A saved layout's zone for a group includes the band above its visible tab.

## Consequences

- A group's window is a band's height shorter than its neighbours (28 px at 100%); the band is
  the group's tab bar, not empty space.
- The probe's tab steps check that every tab has a chip of at least 40 px in a band above the
  window shown, and that the strip follows the group into another zone and back, above it, with
  the hidden tab shown there when clicked.
