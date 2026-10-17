# 0037. A workspace starts an app as Windows' taskbar does, and knows its window by its launcher

- Status: accepted
- Date: 2026-10-16
- Amends: ADR 0022 (sets of apps: an app of the Start menu found by its program)

## Context

The owner, on 2026-10-16: "often when terminal and msys opened, opening a workspace opens another
one ? and asks for powershell or msys (mintty) instead of opening whats described directly".

On his PC (jobs 0184 to 0186, read only): a workspace's Git Bash had been saved from its window,
which mintty.exe shows with the app id `GitForWindows.Bash`; Mosaïk kept mintty.exe as the
program to start it with. Started alone, mintty asks which shell to open (MSYS2, Mingw-w64 32 or
64 bit) and opens a window with no app id, which is not the workspace's Git Bash: each time the
workspace opened, another mintty, another question. Windows keeps, for such a window, the command
its taskbar relaunches it with: `git-bash.exe` for Git Bash, `chrome.exe
--profile-directory="Profile 7"` for a Chrome profile, `steam.exe` for Steam's window (which
belongs to its web helper). And an app added from the Start menu is known by the program its entry
starts (`git-bash.exe`), while its window belongs to another (`mintty.exe`): it was never found
open, and was started again on every open.

## Decision

- **A window's app is started as Windows' taskbar relaunches it**, when the window says how
  (`System.AppUserModel.RelaunchCommand`, read with its app id as windows are scanned); else, as
  before, packaged apps by their id and others by their program file.
- **A window belongs to the program that relaunches it as well as to its own**: an app known by
  `git-bash.exe` or `steam.exe` finds the mintty or web helper window that names it.
- **mintty.exe is never started alone**: a workspace's app saved with mintty.exe as its program
  (before this) starts as `git-bash.exe` when it is Git Bash (its app id), else as mintty with its
  login shell (`mintty -`), which opens bash at once.

## Consequences

- Core tests, failing first: a window its launcher started is found by the launcher (Git Bash,
  Steam); a relaunch command is a program and its arguments (quoted, unquoted with spaces, quotes
  and backslashes as programs read them); mintty alone is never started (core 311).
- `mosaik-probe open-plan` says, without opening anything, which window each app of each workspace
  would take, how each would be started, and the Start menu's terminals.
- Two apps of one kind in a workspace (two Chrome windows) still open two windows: that is what
  the workspace holds.
