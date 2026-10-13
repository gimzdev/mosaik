# 0019. Arrange lays every screen out for its task, and no window gets a zone it cannot fit

- Status: accepted
- Date: 2026-10-12
- Amends: ADR 0014 (zones by name), 0017 (the open workspace's zones are kept; grids)

## Context

The owner, after ADR 0017: "Arrange is better but not good enough. It should assess the main
window (VS Code is superior to a terminal) and make a split view depending on the task, in a
smart, organised way." The workspace open on their screen had three rows, made earlier. ADR 0017
kept an open workspace's zones, so Arrange only filled the rows again. Arrange's own grid, for a
screen without zones, gave every window the same room.

Then, in Quarters: "some windows still can't fit, like Battle.net, it's too tall". Battle.net
will not go below about 1280 x 720, and a quarter of a 1080p screen is 960 x 516. Mosaïk kept
the app's size, so the window spilled over its neighbours.

The owner also asked that apps have proper names ("Applicationframehost" stood for Calculator)
and that the home page list each workspace's apps.

## Decision

- **Arrange guesses the task on each monitor from its windows** (`task::guess`): coding, design,
  writing, browsing, chat and mail, games, or nothing in particular. Each task has the kinds of
  window it is done in, and a table of how much every kind of window belongs to it. The windows
  used last count most (`1 / (1 + 0.6 * rank)`). Games count for less, so a launcher left in
  front does not turn a coding screen into a game's. Claude and ChatGPT are their own kind,
  "Assistant", which helps any task rather than making it "chat".
- **The window the task is done in is the main one** (an editor when coding, a design tool, a
  document), and it gets the most room: 60% of the width beside one window, down to 50% beside
  four. When coding, the terminal used last goes under the editor (32% of its column). The
  windows that help most sit beside it, up to what the screen holds: three on a 1080p screen, or
  four in a 2 x 2 block when all four belong to the task. On an ultrawide screen the main window
  is in the middle; on a screen turned on its side, at the top. Windows that have nothing to do
  with the task are minimised. New zones are named after their windows ("Code", "Terminal").
- **Every monitor with windows gets the layout of its task, in place of the open workspace's
  zones there**, as a layout picked on the home page already did on the main monitor. Arrange
  is Mosaïk's own layout; the workspace keeps the new zones, and Undo puts the old ones back for
  ten minutes. Zones on a monitor without windows stay. Zones named for a kind of app no longer
  draw windows (ADR 0014): a workspace's apps are set on the Workspaces page (ADR 0018).
- **Arranging twice moves nothing.** Arrange does not change the z-order. A window beside the
  main one that already fills one of the areas there keeps it, even when the windows minimised
  in between change which one counts as used last.
- **No window gets a zone smaller than it can go.** Mosaïk asks each app how small its window
  goes (`WM_GETMINMAXINFO`, which Windows carries across processes; `window::least_frame`,
  scaled for apps that are not DPI aware), and takes a window that cannot be resized at its own
  size. The main zone grows to fit its window, up to three quarters of the screen with windows
  beside it. A terminal too tall for under the editor goes beside it. Fewer, taller windows sit
  beside the main one until the ones that help most each fit. A window that fits nowhere is
  minimised, rather than keeping the others away. In a layout picked, only windows that fit one
  of its zones take one, each into a zone it fits; Battle.net in Quarters waits in the taskbar.
- **The home page says the task**: "With Arrange, for coding: 2 windows minimized", and so does
  the message ("Arranged 5 windows for coding."). Protocol 5: `ArrangePreview.task`.
- **Apps have proper names**: a Store app (hosted by `ApplicationFrameHost.exe`) is named by its
  window's title ("Calculator"), else its package's name, spaced ("Windows Calculator");
  `explorer.exe` is "File Explorer". Names missing from older workspaces are filled from their
  open windows when the workspace opens. The home page lists each workspace's apps, up to five
  and how many more, with Apps to change them (protocol 5: `WorkspaceSummary.apps`).

## Consequences

- One click lays out a fresh screen for what the user is doing. A user who wants their own
  zones keeps them in a workspace (opening it puts every app back in its zone), picks a layout,
  or draws zones in the zone editor; Arrange then changes them, with Undo.
- A minimum size is only known when the app says it in `WM_GETMINMAXINFO`. An app that refuses
  a size some other way is still placed at the size it keeps (ADR 0005), and can overlap.
- Measured with `mosaik-probe smart`: 3a (the layout of the task), 3c (arranging twice moves
  nothing), 3d (a new window becomes the main one; the one used longest ago is minimised), and
  3h (a window that will not get as small as a quarter: its minimum read from another process,
  and Quarters minimises it), in `docs/STATUS.md`.
