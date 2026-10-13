# 0022. Sets of apps, and the windows open sorted into workspaces the first time

- Status: accepted
- Date: 2026-10-13
- Amends: ADR 0016 (the home page)

## Context

The owner opens the same apps together many times a day (Windows Terminal, Chrome, Claude, VS
Code and File Explorer) and asked for "presets to open a bunch of apps", proposed from the apps
on the machine rather than set up by hand: "secondary, it shouldn't take all the space, but
visible early." And: "when someone opens Mosaïk for the first time, the open apps should be
sorted into workspaces depending on what makes sense, with the appropriate layout." Then: "make
sure it's clean, concise, non-invasive and easy to understand."

## Decision

- **Mosaïk reads the Start menu's apps** from the shell's Apps folder (`FOLDERID_AppsFolder`):
  each app's name, its AppUserModelID and the program it starts (`mosaik_win::apps::start_apps`),
  on a thread of its own when Mosaïk starts, and again when asked after ten minutes. It stays in
  memory; nothing is written or sent anywhere.
- **A set is the apps installed that go together for a task** (`mosaik_core::sets::suggest`):
  for coding, an editor, a terminal, a browser, an assistant and File Explorer; for design, a
  design tool, a browser, File Explorer and an assistant; for writing, a word processor, notes,
  a browser and an assistant; for chat and mail, two chat apps and a mail app; for games, two
  launchers, a chat app and a media app. Of several apps of one kind, the one open now is taken,
  else the best known. Apps that come with Windows (Edge, Notepad, Copilot, the Xbox app, the
  Teams and Mail that come with it, PowerShell) are taken only when open or when nothing else is
  installed for the role; second tools (Notepad++ beside an IDE, Docker Desktop, a database or
  Git client) only when nothing else is, even when open. Neither takes a second place of a kind
  (a second chat app) or makes a set alone: a set needs an app the task is done in that the user
  chose (an editor, a launcher...), and two apps at least. An entry that has explorer.exe open a
  folder is not File Explorer, Calendar is not a mail app, and a launcher's entry (Battle.net
  Launcher.exe) is known by the program whose window it opens (Battle.net.exe). Three sets at
  most, one row, the ones with the most apps open first.
- **The home page shows them in one line under Arrange**: "Open the apps for", then a button
  per set named after its task ("Coding"), which names the set's apps to screen readers and, on
  hover, to the pointer. One line, so the layouts below stay in view in Mosaïk's window (980 x
  720); cards with the apps written out took two rows there and pushed the layouts out of view
  (job 0112's tour). Nothing shows when Mosaïk finds no set.
- **A click opens the set's workspace** (`sets::workspace_of`): the first workspace with all of
  the set's apps, whatever its name, as the user left it; else a new one, laid out by Arrange's
  planner for the task on the main monitor (ADR 0019, 0021) and named after it ("Coding", or
  "Coding 2" when a workspace has that name with other apps, as the welcome's sorting can make).
  It opens like any workspace: the apps that are not open start (a desktop app through
  `shell:AppsFolder\<its id>`, a packaged app by its id).
- **The first time, the windows open are sorted into workspaces** (`sets::groups`): when the
  user clicks Get started, each task whose own kind of window is open takes those windows, every
  other window joins the task it belongs to most (at least 0.45 by Arrange's table), and each
  task with two windows or more becomes a workspace laid out for it, named after it. No window
  moves. A workspace of that name already there is left as it is. One message says what was made
  ("Your open windows make 2 workspaces: Coding and Games."); when nothing was, nothing is said.
- **Protocol 6**: `sets`, `open_set` and `sort_windows`, and `mosaikctl sets`, `open-set` and
  `sort-windows`.

## Consequences

- Measured on the owner's PC (job 0110, `mosaik-probe sets`): Mosaïk's reading of the Start menu
  and Windows' own list (`Get-StartApps`) give the same 139 apps, read in 163 ms. Its first
  choices there took a folder of the Windows SDK, opened by explorer.exe, for File Explorer,
  offered two Teams and Calendar as chat and mail, and would have taken Notepad++ over VS Code
  because it was open: the rules above come from that run.
- Only apps in the Start menu and of a kind Mosaïk knows (`arrange::Kind`) are offered: a
  portable app, or one Mosaïk has no kind for, is not.
- A set finds its workspace by its apps: renamed, it is still the set's; with one of the set's
  apps taken out, the next click makes a new one.
- Sorting happens once, at the end of the welcome. Later, Arrange lays out what is open, and Save
  layout keeps it.

## Amended by ADR 0023 (2026-10-13)

A set's new workspace shows three windows at most; the set's other apps are tabs with the app
they go with, so all of them open. The writing and design sets have no assistant. An app of a set
that is open is used, never started again, packaged apps included (their windows take their
process's AppUserModelID), and opening a set twice while its apps start starts each once. A
started app takes its zone as soon as its window shows, also when it shows cloaked first.
