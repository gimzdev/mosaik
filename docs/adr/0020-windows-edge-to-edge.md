# 0020. Windows meet edge to edge, without coloured rings

- Status: accepted
- Date: 2026-10-12
- Amends: ADR 0002 (zone frames drawn by Mosaïk), 0011 (the ring alone)

## Context

Since 1.0, every window in a zone wore a 2 px ring in its zone's colour, and windows sat apart
by the width of two rings (8 px, then 4 px), so that the rings did not cover each other. The
owner, having asked several times for no space to be lost: "get rid of the coloured borders, it
looks childish and we lose space. Just window to window."

## Decision

- **No rings by default, and no space between windows**: windows meet edge to edge, and reach
  the screen's edges (`FrameStyle::gap` 0). A window still wears its zone's tag in its title bar
  (the tag takes no room), and the zones still show, in their colours, while a window is dragged.
- **Rings stay as a setting** ("Coloured borders"). With rings on, windows sit at least two rings
  apart (`FrameSettings::effective_style`), so a ring never covers the next window. A space
  between windows set in `settings.json` is kept as set.
- **Settings format 5.** Files from 1.0 and from development builds of 1.1 move forward once: the
  space every older version kept by default (8 px, then 4) becomes 0, and rings go off. A space
  the user chose stays. One note says what changed and how to bring the borders back. A style
  saved with values missing takes the defaults for those values, rather than being refused.
- **The app's picture of the screen and the site's illustrations agree**: windows edge to edge,
  with their tags and without rings.

## Consequences

- Windows 11 rounds window corners, so a little of the desktop shows at the corners where four
  windows meet. Squaring other apps' corners is not attempted.
- Nothing marks a window in a zone but its tag. A window whose title bar has no room for even
  the zone's initial shows no sign of its zone, unless rings are turned on.
