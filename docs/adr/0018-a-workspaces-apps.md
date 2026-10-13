# 0018. A workspace's apps, listed and edited on the Workspaces page

- Status: accepted
- Date: 2026-10-12

## Context

The owner: "we must be able to see and set the list of current apps in a workspace and add /
remove some easily". Until now a workspace's apps changed only on screen: by saving, Arrange, a
drop into a zone, or a window moved by hand. Nothing showed which apps a workspace held.

## Decision

- **Each workspace on the Workspaces page has an Apps button** that shows its apps under its
  row: each app's name, a list of the workspace's zones to put it in, and Remove. "Add a window"
  lists the windows on screen that are not part of the workspace (app and title); picking one
  adds its app, in the workspace's first empty zone, else where the window is.
- **In the open workspace, a change moves the window at once** (the user asked for it): an app
  added or moved goes into its zone; a tab moved to another zone leaves its group; an app
  removed keeps its window where it is (a background tab comes back). In a workspace that is not
  open, the change waits until it opens.
- **Pure edits in `mosaik-core`** (`editing::add_app`, `move_app`, `remove_app`, `empty_zone`),
  tested on their own; the engine checks, saves, then moves (`workspace_apps.rs`). A change that
  cannot be saved changes nothing and says why.
- **Apps have names.** A slot keeps its app's name from when its window joined (`Slot::label`,
  optional in the file, so older workspaces load as they are); for one without, the name comes
  from its open window, its program's description, or its program's file name.
- **Protocol 5**: `apps`, `add_app`, `move_app`, `remove_app`, each answered with the list as it
  is now (`Reply::Apps`); `mosaikctl apps`, `add-app`, `move-app` and `remove-app` do the same
  from a terminal.

## Consequences

- The list of windows to add is read when the panel opens, when Mosaïk's window comes back to
  the front, and after every change.
- Two apps with the same name (two Chrome windows) show twice under that name; their zones tell
  them apart.
- Measured with `mosaik-probe smart` (3g), in `docs/STATUS.md`.

## Amended by ADR 0023 (2026-10-13)

The Workspaces page is part of Home: a workspace's apps open from its ⋯ menu, "Change its
apps…", and close from the same menu, "Hide its apps".
