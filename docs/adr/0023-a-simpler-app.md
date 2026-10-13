# 0023. A simpler app: two places, three windows to a view, every app started once

- Status: accepted
- Date: 2026-10-13
- Amends: ADR 0015 (the quick menu), ADR 0016 (the home page), ADR 0018 (a workspace's apps),
  ADR 0022 (sets of apps)

## Context

The owner, after using 1.1 for a day: Mosaïk is "way too complicated: simplify without losing
value and functions". And:

- "Shouldn't open an app twice: if it's open, take the one opened." Opening the coding set
  started a second Windows Terminal; clicking a set twice started its apps twice.
- "When opening the code preset it doesn't organise right away: I have to open, then organise."
- "We should really rarely use more than three windows in a view."
- "For writing and design, why would someone need Claude? It shouldn't be in the preset."
- "When clicking another tab of a zone, the tabs often go behind the new window."
- "Opening the options of the last one on the list doesn't show them: open the menu upward, or
  scroll down."
- "The side button has to be pixel perfect: when we touch the border of the screen it doesn't
  take the input." The owner's main screen has two screens to its right, so the pointer goes on
  past the right edge, where the quick menu's line was.
- "When playing an FPS, going sideways should never open the side bar."

Measured causes: Windows Terminal's windows carry no AppUserModelID of their own, so a set's
Terminal, known by its id, never matched the one open. Chrome, VS Code, Claude and Windows
Terminal show their first window cloaked and uncloak it once drawn: a window started for a
workspace was looked for when it showed, while still cloaked, and never placed.

## Decision

- **Two places**: Home and Settings, in a bar along the top of Mosaïk's window. The Workspaces
  page is part of Home: each workspace is a row with its apps at a glance and Open; everything
  else is in its ⋯ menu (Change its apps…, Edit zones, Add or Change shortcut…, Rename…,
  Delete…). The zone editor opens from there and goes back there. About is the last section of
  Settings. Settings shows what most people change (the switcher's shortcut, starting with
  Windows, the theme, snapping, zone tags, coloured borders, the quick menu); the rest is under
  More settings. Zone tags are one choice (in title bars, above windows, off; off hides the tags
  and leaves tabs where they were), and so is the quick menu (right edge, left edge, off).
- **A menu opens where its items can be seen**: below its button, or above it when only there
  do they fit in the window; with room neither way, the page scrolls to it. A page opens at its
  top.
- **Three windows to a view at most** (`mosaik_core::task::MOST_IN_VIEW`): Arrange's planner and
  a set's new workspace put three windows at most on a monitor. Arrange still never makes tabs
  (ADR 0017): its other windows are minimised. A set's other apps go as tabs with the app they
  go with (`task::tabs_for`, `goes_with`: a chat app with mail or the browser, File Explorer
  with the terminal...), one extra per zone beside the main one, so the set's apps all open. The
  writing and design sets have no assistant.
- **Every app is started once**: a window of a packaged app that has no AppUserModelID of its
  own takes its process's (`GetApplicationUserModelId`); apps are the same when their ids are
  (without case), else when their programs are (`AppMatch::same_app`). Opening a workspace again
  while its apps are starting keeps those starts: each app started stands for one slot, and its
  window takes it.
- **A started app takes its place as soon as its window shows**, also when the window uncloaks
  or comes back from the taskbar.
- **A tab's strip stays above its window**: after a tab is clicked, the strips and frames are
  put back above their windows 120 ms and 400 ms later, as some apps raise their window again
  once it shows.
- **The quick menu's line is where the pointer stops** (`mosaik_core::edge::handle_edge`): on
  the edge chosen, when the main screen's edge there stops the pointer; else on the other edge
  when that one does, which Mosaïk says once, ever, in its window the next time it is opened (a
  note, `UiEvent::Note`: it does not open the window, since Mosaïk starts with Windows); with
  screens on both sides, on the edge chosen.
  **It never answers a game**: the line does not open the menu while an app keeps the pointer
  inside part of the screens (`input::pointer_confined`), or while the window in front, covering
  the main monitor's work area (a game in a window, maximised), hid the pointer the last time it
  was seen over it: the pointer is looked at five times a second while such a window is in front
  (`Engine::pointer_tick`), never otherwise. Over the line itself the pointer is Mosaïk's own and
  shows, whatever the game did with it (Windows keeps a pointer per program), so it is not
  looked at there. In a game's menus, with the pointer showing, the line answers.
- **The site** says the same: a line beside the title (Mosaïk suggests sets of apps that go
  together and opens them in tidy views; you can make your own), three windows to a view in the
  hero's desktops, a thin line of desktop between windows so each reads as its own, and on the
  download page that the file is Microsoft's small Store installer (ADR 0008, which the owner
  kept over an installer of Mosaïk's own).

## Consequences

- Nothing was removed: every action of the Workspaces and About pages is in Home's ⋯ menus and
  at the end of Settings.
- The workspace probe's started window shows cloaked first, and the workspace is opened twice
  before it shows: one window must start, and take its zone (`mosaik-probe workspace`, step 5).
  The smart probe's game window hides the pointer over the work area: the line must not open
  (`mosaik-probe smart`, 4g). Both run against the engine without these changes first.
  The tour reaches a workspace's ⋯ menu by focusing it, as the keyboard does, since the list is
  below the fold.
- A game in a window that does not cover the main monitor's work area, or that is made to cover
  it after it came to the front, is looked at only from the next time it comes to the front.
  Games that turn the view with the mouse usually confine the pointer too, and a game in full
  screen hides the line anyway (ADR 0015).
- A workspace saved with more windows side by side keeps them: three to a view is how Mosaïk
  lays out, not a limit on what the user makes (a layout such as Quarters has four zones).
