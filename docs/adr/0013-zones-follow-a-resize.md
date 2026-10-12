# 0013. Resizing a window in a zone moves the lines between zones

- Status: accepted
- Date: 2026-10-12

## Context

The owner: "when stretching a window in a zone it should stretch the zone as well and
surrounding zones, like smart way". In 1.0 a resize never touched zones (ADR 0010): a window
resized by hand was remembered at its new size, over its neighbours, and went back into its
zone the next time the workspace opened. Changing the zones themselves meant opening the zone
editor.

## Decision

- **Lines between zones** follow the zone editor's rule (`mosaik_core::lines`, the same as
  `apps/desktop/src/editor/model.ts`): a line is a stretch of edge that zones on both sides
  share, and moving it moves every zone along it, so zones stay tiled, never overlap and never
  get smaller than the editor allows (5% of the monitor each way).
- **A resize by hand** of a window of the open workspace that fills a zone (its place in the
  workspace is that zone; a window that only sits over a zone does not count) moves the line
  under each side that moved more than 2 px, by as much as the side moved, as a fraction of the
  monitor's work area (`follow_resize`). The line is carried on through places where four zones
  meet (`grid_line_of`): in quarters, the whole middle column moves, as a spreadsheet's column
  does, so a grid stays a grid. A side on the monitor's border has no line: nothing moves
  there, and the window goes back to its zone when the button is let go.
- **While the resize lasts**, the zones of that monitor show as they will be, with the same
  preview a drag shows, the window's own zone lit.
- **When it ends**, the layout is saved with the new areas (as a save from the zone editor) and
  the windows in every zone that changed are fitted to it, the resized window included.
- **Apps that keep a larger size push back, once.** A browser keeps about 500 px; a zone made
  narrower than that would leave the app over its neighbour. Each window that kept a larger
  frame than its new zone moves its zone's lines out by as much, once, and the zones that
  changed again are fitted. One pass, so two stubborn apps can never pull a line back and forth.
- **Shift resizes freely**, as it moves freely (or the reverse, when the setting swaps them):
  the window is remembered where it is and the zones stay. The setting "Zones follow a resized
  window" (on by default) turns the whole of it off; so do snapping off and safe mode.
- The resize is told from a move by Windows' resize arrows (ADR 0010), so it works for any app
  that resizes through Windows' frame.

## Consequences

- Resizing changes the open workspace, as an edit in the zone editor does: to undo it, resize
  back, or use the editor. Arrange's Undo (ADR 0014) does not cover it.
- An app that resizes its window with its own code (no move-size loop) is not followed: its
  zone stays.
- A window that is a group's visible tab moves its group's lines; the other tabs follow when
  shown.
- Measured with `mosaik-probe smart` (`docs/STATUS.md`).
