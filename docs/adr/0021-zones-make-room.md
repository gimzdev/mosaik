# 0021. Zones make room for the windows in them

- Status: accepted
- Date: 2026-10-13
- Amends: ADR 0005 (apps that keep their own size), 0019 (a window that fits nowhere is
  minimised)

## Context

After ADR 0019, a window that would not get as small as any zone of a layout waited in the
taskbar: Battle.net (about 1280 x 720 at least) in Quarters (960 x 516 each on a 1080p screen).
In a zone made by hand or by a drop, it kept its size and spilled over its neighbours. The owner:
"instead of letting it overlap or minimising the app, can we reorganise the size of the zones?
It's the whole point of the app." And: "not becoming the main zone, just taking the minimal space
it needs, even if it's big"; "increasing a wide or tall zone doesn't mean making it the main
window."

## Decision

- **A zone too small for its window grows just enough, and the zones beside it make room**
  (`mosaik_core::fit::make_room`). Zones that tile a monitor in rows and columns (every layout
  Mosaïk makes, and most drawn by hand) are read as a tree of cuts: the monitor cut into columns,
  each column into rows, and so on. Each part keeps its share of the room it is cut from, unless
  one inside it needs more; then the parts beside it give up the difference, in proportion to
  what each can spare. A zone gives up room down to what its own windows need, and never below
  320 x 200 (or the room it had, if it had less). When the zones can be cut either way first
  (quarters are two columns of two, or two rows of two), the way that moves the zones least wins:
  Battle.net in Quarters makes the top row 720 px tall and its quarter 1280 px wide, and leaves
  the bottom row's two quarters as they were. Zones that already fit are not touched.
- **The window keeps its place in the layout.** A big window beside the main one widens its
  column and the main zone gives up the room; the main window stays the one the task is done in.
- **Arrange** fits each layout it considers before judging it: the main window, the terminal
  under the editor (at least 260 px tall there) and up to as many windows beside it as the
  screen holds, among the eight that help most, the ones that help most first. ADR 0019's cap
  (the main zone up to three quarters of the screen) is gone: the main zone grows as its
  neighbours allow.
- **A layout picked on the home page** puts each window in the zone its kind suits, then the
  zones make room (`arrange::make_room_for`). When they cannot, two windows trade zones; when
  that does not do, the window that needs the most room is minimised, and the zones try again.
- **The engine makes room from what windows did** (`mosaik-engine/src/room.rs`): after opening
  a workspace, Arrange, a drop, an app added or moved, or a display change, a window that kept a
  larger size than its zone (an app that refuses a size without saying so in
  `WM_GETMINMAXINFO`) gets the room it kept, once per action. The workspace keeps the new zones.
- **A window is minimised only when the windows cannot all fit**: two 1280 px windows in Halves
  on a 1920 px screen, or a window larger than the screen beside others.

## Consequences

- A workspace's zones follow the windows in it: they keep the room they made, saved with the
  workspace, until the user resizes them, picks a layout or edits them.
- Zones drawn so that no line runs right across them (a pinwheel) are not changed; a window too
  big for its zone there keeps its size, as before (ADR 0005).
- A zone whose app says no minimum can be made as small as 320 x 200 by a big neighbour.
- Measured with `mosaik-probe smart`: 3h (Quarters with a window that needs 60% of the screen
  each way: its quarter grows, every window fits, the zones still fill the screen, and of five
  windows for four quarters only one other is minimised) and 3i (a tall window dropped into the
  smallest zone: the zone grows, every window fits), in `docs/STATUS.md`.

## Amended by ADR 0032 (2026-10-16)

A window the zones cannot make room for beside the others stacks behind a window of a zone it
fits; it is minimised only when it fits no zone at all.
