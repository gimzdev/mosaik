# 0017. Arrange minimises what it has no zone for; windows move only when the user asks

- Status: accepted
- Date: 2026-10-12
- Amends: ADR 0011 (strips), 0014 (windows left over), 0015 (rows, timing), 0016 (the picture)

## Context

The owner, using 1.1 on a screen with eleven windows and a workspace of three zones: "arrange
window (auto) is really fucked up". Arrange had put all eleven in the three zones, eight of them
as tabs of one group (ADR 0014: windows left over join the window that suits them as a tab).
Also: "the tab system is inconsistent, I mean the position of tabs", "when Mosaïk is active and
alt-tabbing it resizes the app for a zone sometimes; it should never resize when opening a
window, only when creating a scene or drag and drop", and the quick menu should open at once,
smoothly, with fewer rows.

The data (job 0104) showed why windows moved on Alt+Tab: a window brought forward has its title
bar read again (ADR 0011), and when the reading changed its mind about the window's band, the
window moved into or out of it, minutes after anyone placed it. Strips sat in a band or in the
title bar depending on each reading, so they moved from window to window.

## Decision

- **Arrange never makes tabs.** The open workspace's zones are kept. A monitor without zones gets
  zones for its windows: the presets of ADR 0014 up to four windows; for more, a grid with as
  many zones as there are windows, up to what the monitor holds well (columns at least 600 px
  wide, rows at least 420 px tall, at most 4 by 3; the wider zones in the top rows,
  `presets::zones_for_windows`). When such a monitor (or one a layout was picked for) has more
  windows than zones, **the windows used last take them**, whatever their kind (job 0105: the
  window in front, Claude, was minimised for editors used long before). Windows left over take
  an empty zone on their monitor; with every zone taken they are **minimised**, so they stay in
  the taskbar, and do not join the workspace. Undo restores them where they were.
- **A window already in its zone scores +100** (was +45): arranging again keeps it there, even
  when another window would suit the zone a little better.
- **Windows move only on the user's actions**: Arrange or a layout, opening or saving a
  workspace, a drop, the zone editor, a resize by hand, a tab clicked, an app added to or moved
  in a workspace. A window's band (ADR 0011) changes only within 3 seconds of such an action
  (`titlebar::PLACING`, `Engine::acted`), while the window is being placed; a reading taken
  later (a window brought forward, its title changed) never moves it.
- **A tab group's strip is always in its visible tab's title bar**, at its right end: in the
  widest stretch with room for it, else over the end of the last stretch, else just left of the
  caption buttons (`TitleRoom::strip_spot`). Strips in bands, for windows whose title bar never
  answers, are right-aligned too. Tabs sit in the same place on every window, and no window
  moves for its strip.
- **The quick menu** opens 60 ms after the pointer stops on its line, sliding in from the edge
  over 150 ms (ease-out) and fading out over 110 ms, at once when Windows shows no animations
  (`SPI_GETCLIENTAREAANIMATION`). Its first row is "Mosaïk · <workspace>" with "Open": a click
  opens Mosaïk's window, a drag moves the menu along its edge (the handle, under the menu that
  opens at once, can hardly be grabbed any more). A move Windows reports only because the menu
  moved under a still pointer (its slide) is not the pointer coming back: picking a row closes
  the menu even when the pointer stays where it was (job 0105). Then Arrange windows; Switch workspace with its shortcut; Save layout;
  Edit zones; Snap while dragging, On or Off. Undo arrange is no longer a row: it is on the home
  page, next to Arrange.
- **The home page's picture** shows a window Arrange would minimise shrinking to the taskbar,
  and its caption says how many ("With Thirds: 1 window minimized"). Protocol 5:
  `PreviewWindow.minimized` replaces `tab`.

## Consequences

- With more windows than zones, the ones Arrange keeps are the ones that suit the zones best (ADR
  0014's scores); the others wait in the taskbar. A user who wants them as tabs drops them on a
  tag, as before.
- A window whose title bar changes after it was placed (an app that redraws its own) keeps the
  band, or lack of it, it was placed with until the next action of the user's places it again.
- Measured with `mosaik-probe smart` (3d, 3e: minimised windows; 3f: windows brought to the front
  move 0 px; 4b: the menu open in 250 ms or less, its rows) and `mosaik-probe workspace --drag`
  (the strip in the same place for two apps), in `docs/STATUS.md`.

## Amended by ADR 0027 (2026-10-15)

Strips are no longer in title bars: a tab group sits below a band with its strip, and no window
moves into or out of a band after a title bar is read.

## Amended by ADR 0029 (2026-10-15)

The quick menu's Save row says what it does ("Update “Email”", or "Save as workspace") and a toast
confirms the save. A press in the 300 ms after the menu opened by itself does nothing.
