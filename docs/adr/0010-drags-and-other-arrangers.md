# 0010. Telling a move from a resize, and living with Windows' snapping and other arrangers

- Status: accepted
- Date: 2026-10-12

## Context

Mosaïk follows a window's drag through Windows' move-size loop (`EVENT_SYSTEM_MOVESIZESTART`
and `END`): zones and tags light up during a move, never during a resize. The first version
called a drag a resize when, at the moment Mosaïk noticed its start, the pointer was not inside
the window away from its edges. The app probe (`mosaik-probe workspace --apps`, job 0047) found
that wrong for every app dragged quickly upwards by its title bar: Windows starts the loop only
once the pointer moves, and by the time the event arrived the pointer had left the window's top.
No tag accepted the window, and dropped at the top of the screen, Windows maximised it instead.

Windows' own snapping ("Snap windows", on by default) reacts to the same drags at the edges of
the screen, and so do other programs: tiling managers (komorebi, GlazeWM, workspacer, FancyWM),
zone snappers (PowerToys FancyZones, AquaSnap) and tab groupers (WindowTabs).

## Decision

- **Move or resize** is decided by what Windows shows and what the window does, not by where
  the pointer was (`mosaik_core::snap::DragKind`). Windows shows one of its resize arrows for the
  whole of a resize, mouse or keyboard, whichever app the window belongs to: that is a resize.
  Otherwise (a pen, an app that draws its own pointer, a cursor tool) the first change of the
  window decides: same size, a move; a new size that kept one side and the top or the bottom
  where they were, a resize by an edge or a corner; a new size with every side moved, a
  maximised or snapped window taking back its size as it is dragged, a move. Only with no
  pointer on screen (touch) does the position test decide the start.
- **Apps that drag their own window** (some custom title bars: Java, Qt, launchers, games)
  move it themselves instead of through Windows' move loop, so no move-size event comes. While
  zones are on, a window that takes the mouse (`EVENT_SYSTEM_CAPTURESTART`, a press) is
  watched: if it moves at the same size while the primary button is down and no move loop
  started, its app is dragging it, and Mosaïk follows it like any drag. It ends when the app
  lets the mouse go with the button up, or 400 ms after the button is up if the app keeps the
  mouse. Moves are hooked only for the app holding the mouse, and only while it holds it.
- **Windows' snapping is left on and not fought.** Measured on Windows 11 25H2 (job 0048): a
  window dropped on a tag at the screen's top edge or top centre becomes a tab and is not
  maximised; one dropped on a zone at the left edge lands in the zone, 0 px off. Mosaïk acts
  after Windows at the end of the drag, so the place the user dropped on wins. Windows still
  shows its own preview during such a drag, so the first time a drop that went onto a zone or a
  tab was at an edge where Windows' snapping reacts (left, right, top; the bottom only at its
  corners) while it is on, the user is told once, ever, why, and where to turn Windows'
  snapping off if they prefer. Mosaïk never changes that setting itself.
- **Other arrangers** are found by their program in one snapshot of the process list of the
  user's own session, when Mosaïk starts and once things settle after a switch or a display
  change (never during a drag or before a reply), and each is named to the user once, ever, with
  what to do: pause one of the two, give the other the drags made with Shift, or use one of them
  for tabs (`mosaik_core::others`). A zone snapper is named only while Mosaïk's own snapping is
  on. Once every known one has been named, the process list is not read again. Mosaïk does not
  stop or configure them.
- Notices told once ever are kept in `settings.notices_shown`; the interface saving settings it
  read earlier cannot bring one back. While the settings file could not be read (written by a
  newer Mosaïk, say), they are kept for the session only: Mosaïk never writes its defaults over
  the user's settings on its own.

## Consequences

- Quick upward drags onto tags work for every app in the probe's catalog that uses Windows'
  move loop the usual way. Chromium-based apps start the loop themselves once they see the
  pointer move inside their window, and treat a drag that starts along their top few pixels as a
  resize, as they do without Mosaïk.
- A window Windows also snapped keeps Windows' "arranged" mark (`IsWindowArranged`): dragged
  again, Windows first gives it back its size from before. Mosaïk's next drop sizes it anyway.
- The known-program list is short and needs updating as tools come and go; a program Mosaïk
  does not know is not named, and the two may still fight over a window.
