# 0005. Placing windows by their visible frame

- Status: accepted
- Date: 2026-10-11

## Context

Zones are drawn by Mosaïk and must line up with the windows in them: no gaps from Windows'
invisible resize borders, no windows hanging off screen, and the same result on every monitor
and DPI.

## Measurements

`mosaik-probe matrix`, eight apps, four targets each plus "restore from maximised into a zone"
(2026-10-11, `docs/STATUS.md`). Every app had invisible borders of 7/0/7/7 px (left, top, right,
bottom) at 100%.

First run, with an asynchronous restore and no size handling:

- Chrome, restored from maximised into the left half, ended 7 px inside its target: the
  asynchronous restore landed after the corrective move.
- VS Code, restored from maximised into the centre, moved itself back to its previous place
  (982 px away) after about 300 ms.
- Chrome and Edge will not go below 502 px wide, Windows Terminal below 466 px: in a 420 px zone
  at the right edge they hung 82 px and 46 px off the screen.

After the changes below: 0 px error on all 40 placements, with no corrections needed.

## Decision

- Mosaïk places the visible frame (`DWMWA_EXTENDED_FRAME_BOUNDS`), adding the window's current
  invisible borders, measured at the moment of the move; a window that changes DPI on the way is
  placed a second time with its new borders.
- Restoring a maximised or minimised window into a zone is synchronous, into a first guess of
  its normal borders, then measured and corrected. Each window is moved on a thread of its own,
  so a hung app holds up only its own move (`mover.rs`).
- A window that keeps a size of its own is fitted with `fit_size`: centred in the zone if smaller,
  otherwise kept on the zone's edge nearer the screen edge so it grows toward the middle of the
  screen, and always inside the work area.
- A window without a frame to resize it by (a game, a dialog) is never resized, only moved:
  `fit_size` with its own size (ADR 0012).
- After a move the engine watches the window until the app stops moving it, and puts it back at
  most twice if it wandered off.

## Consequences

- A zone narrower than an app's minimum overlaps its neighbour; the zone editor warns when a zone
  is narrower than 500 px.
