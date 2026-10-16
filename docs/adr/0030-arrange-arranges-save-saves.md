# 0030. Arrange arranges, Save saves

- Status: accepted
- Date: 2026-10-15
- Amends: ADR 0014 (Arrange keeps its zones in a workspace), 0029 (what Arrange says)

## Context

Since ADR 0014, Arrange kept its zones in a workspace, so tags, snapping and tabs had zones to
work with: the open one took the new arrangement, and with none open a new one was made
("Workspace 1"). The owner's list showed what that does to someone new to Mosaïk: "Workspace 1"
made by Arrange and "Workspace 2" made by Save from the same screen. ADR 0029 made Arrange say
so ("“Email” now opens this way", "Saved as a new workspace"), and left the question open. Asked
"Should Arrange only tidy the windows, and only Save as workspace create or change a workspace?",
the owner, on 2026-10-15: "arrange should arrange, save should save. yes".

## Decision

- **Arrange changes no workspace.** It makes none and changes none; it says only what it did
  ("Arranged 4 windows. 1 window minimized."). Since it writes nothing, a workspaces file that
  cannot be written (one from a newer Mosaïk, one Windows will not let it read) no longer stops
  it; only saving is refused then.
- **What it puts on screen is kept apart** (`Arranged` in the engine): its zones, and the
  windows in them, under an id no saved workspace has (`WorkspaceId::arranged()`, `~arranged`;
  a file edited by hand that gives a workspace that id has it changed on load). Tags, snapping,
  tabs, zones that follow a resize and zones that make room use them as they use an open
  workspace's, and a window moved by hand or dropped into one of them changes them, never the
  disk. They last until a workspace is opened or saved, and are gone when Mosaïk quits.
- **The workspace in use stays so.** Arranged from "Email", "Email" is still marked On screen
  (Home, the side menu, the switcher, the tray), its menu still offers "Replace with what's on
  screen", and the side menu's Save row still says "Update “Email”". "Open again" puts Email's
  windows back where it keeps them. Edit zones in the side menu waits for a saved workspace's
  zones to be on screen; a workspace's own Edit zones edits its saved zones, which then move no
  window.
- **Save keeps Arrange's zones** (`mosaik_core::workspace::saved_over`), which are saved nowhere
  else. Saved over a workspace, they take the place of its zones, under its id, name and
  shortcut, as when Arrange changed the workspace itself (its own zones would have left the
  arranged windows over zones they did not fill). Saved as a new workspace, only the zones a
  window sits in go with it, and no shortcut: an empty zone on another monitor would have that
  monitor's windows minimised whenever the new workspace opens. Without Arrange's zones on
  screen, saving is as it was: a workspace saved over keeps its zones, a new one is made from
  the windows alone.
- **Undo** still puts back what Arrange or opening a workspace did, for ten minutes, until a
  workspace is saved or edited: every window where it was, and what was on screen before
  (Arrange's zones, a workspace, or nothing).

## Consequences

- `mosaikctl state` gives `~arranged` as `active_workspace` after Arrange, and `zones` reads
  Arrange's zones by that id; the list (`mosaikctl list`, the `workspaces` reply) marks the
  workspace in use.
- The `smart` probe checks that Arrange made no workspace and says no workspace's name, then
  saves what it put on screen as "Probe arranged": the same zones, no window moved, one
  workspace, open; that workspace's apps are then edited as before.
- Core tests: saved over a workspace, Arrange's zones replace its own under its name and
  shortcut; saved as a new one, only the zones a window is in go (on two monitors, the empty
  one stays behind), without a shortcut; without Arrange's zones, the workspace saved over is
  the base, as before; no saved workspace gets Arrange's id, and one given it by hand gets
  another. Engine test (Windows): with a workspaces file from a newer Mosaïk, Arrange and its
  preview go on and only Save is refused.
- Opening a suggested set after Arrange leaves Arrange's zones behind for good: a set started is
  not undone (ADR 0029), and Arrange's zones were not saved.

## Amended by ADR 0032 (2026-10-16)

The button that arranges says Move windows, and Mosaïk then says "Moved 4 windows." (or "Moved 4
windows into Thirds."); windows left over stack in the zones rather than being minimised.
