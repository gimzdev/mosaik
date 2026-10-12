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
  Otherwise the first change of the window decides: same size, a move; new size in the same
  place, a resize from the keyboard; new size somewhere else, a maximised or snapped window
  taking back its size as it is dragged, a move. Only with no pointer on screen (touch, pen)
  does the old position test decide.
- **Windows' snapping is left on and not fought.** Measured on Windows 11 25H2 (job 0048): a
  window dropped on a tag at the screen's top edge or top centre becomes a tab and is not
  maximised; one dropped on a zone at the left edge lands in the zone, 0 px off. Mosaïk acts
  after Windows at the end of the drag, so the place the user dropped on wins. Windows still
  shows its own preview during such a drag, so the first time a drop on a zone or a tab is at
  the edge of the screen while Windows' snapping is on, the user is told once, ever, why, and
  where to turn Windows' snapping off if they prefer. Mosaïk never changes that setting itself.
- **Other arrangers** are found by their program in one snapshot of the process list, when
  Mosaïk starts and after each switch (never during a drag), and each is named to the user once,
  ever, with what to do: pause one of the two, give the other the drags made with Shift, or use
  one of them for tabs (`mosaik_core::others`). Mosaïk does not stop or configure them.
- Notices told once ever are kept in `settings.notices_shown`; the interface saving settings it
  read earlier cannot bring one back.

## Consequences

- Quick upward drags onto tags work for every app in the probe's catalog that uses Windows'
  move loop the usual way. Chromium-based apps start the loop themselves once they see the
  pointer move inside their window, and treat a drag that starts along their top few pixels as a
  resize, as they do without Mosaïk.
- A window Windows also snapped keeps Windows' "arranged" mark (`IsWindowArranged`): dragged
  again, Windows first gives it back its size from before. Mosaïk's next drop sizes it anyway.
- The known-program list is short and needs updating as tools come and go; a program Mosaïk
  does not know is not named, and the two may still fight over a window.
