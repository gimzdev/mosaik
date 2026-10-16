# 0029. For people new to Mosaïk: say what happens, ask less, wait less

- Status: accepted
- Date: 2026-10-15
- Amends: ADR 0011 (tags cut short), 0015 and 0017 (the side menu), 0016, 0022, 0024 and 0028
  (Home), 0014 (what Arrange says)

## Context

The owner, on 2026-10-15: "lets review the app entirely for efficiency, optimizations, and
cleanness in order for the use of the app for complete boomers be quick and effective". Three
reviews read the code and walked the window against its stand-in engine (25 findings on use, 18
on speed, 27 on the code). The owner's own data showed one of them: "Workspace 1" and
"Workspace 2" in his list, made by Arrange and by Save layout from the same screen.

The PC run of the same day (job 0150) found a bug: a new build asking the Store's 1.0.1 to quit
posted a close to all nine of its windows; the window its interface's events go through closed
too, so it logged "asked to close", then "the interface thread is gone", and kept running
without a way to quit.

## Decision

**Mosaïk's window**
- Text is 16 px (15 px small, 13 px captions) and every control on Home and a workspace's page is
  at least 40 by 40 px. Ctrl and + or - zoom the window.
- The picture says it is a preview ("Nothing moves until you press Arrange."), and says how to use
  it ("Drag a window onto another to swap them."); Reset is "Undo swaps".
- "Smart" is "Automatic". Five layouts show with their names (Halves, Thirds, Main + side, Half +
  stack, Quarters: the side menu's five); the other five are under More layouts.
- A workspace clicked is shown as "Preview of “Coding”", with Open and Back (Esc too); a second
  click keeps it picked; a double-click opens it. A suggested set is shown the same way, with
  "Start these apps", rather than starting its apps on a click.
- What an action did shows in one place, the line under the picture. The workspace on screen is
  marked "On screen" (not "Open now", which read as a button), there and in the side menu and the
  switcher. "Save layout" is "Save as workspace"; a workspace's menu says "Replace with what's on
  screen".
- The first time, one line says what Mosaïk does and where its side menu is, and the workspaces
  made from the windows open are said to be so, with how to delete them.
- Settings start with what everyone sets (start with Windows, theme, the shortcut, snapping, zone
  numbers, coloured borders, the side menu); where tags go, what hidden tabs do and Shift's role
  are under More settings.
- The window reads from the engine once per burst: focus, a change and its own action, together,
  make one read about 150 ms after the last of them, never two at once.

**What the native parts say and do**
- Arrange says what it did in short sentences, and which workspace now opens this way ("Arranged 4
  windows. 1 window minimized. “Email” now opens this way.").
- The side menu's Save row says what it does ("Update “Email”", or "Save as workspace"), and the
  menu and the switcher confirm a save in a toast.
- A press on the side menu in the 300 ms after it opened by itself (the pointer resting on its
  line) does nothing: a browser's scroll bar runs along the same edge, and the click meant for it
  would otherwise land on a row and rearrange the windows. The menu still opens at once.
- A zone's tag shows its whole number: with little room, without its side padding (compact),
  never its first digit alone ("1" for zone 12 names another zone).

**Quitting**
- A newer Mosaïk asks an older one to quit through the older one's engine window alone
  (`window::ask_mosaik_to_quit`).
- A request to close posted to any window of the engine's thread (the side menu's line, a frame)
  goes to the engine's window: `taskkill` without `/F` asks only the windows that show, which the
  engine's never does (job 0152: Mosaïk in the tray did not quit).
- When the interface cannot be reached as Mosaïk is asked to close (every window it has closed,
  the one its events go through among them), Mosaïk quits from its engine side, every window it
  hid coming back first.

**Less waiting**
- A scan reads only the windows Windows shows (23 of 326 on the owner's PC); the ones Mosaïk hid
  stay shown to Windows (cloaked, minimised or off screen), and classification left the others
  out first anyway.
- Waits for the threads that move, hide and show windows end when the last answer comes (an event
  the threads set), not at the next 5 ms look, which Windows rounds up to its 15.6 ms tick.
- Opening the switcher no longer waits for the next frame only to log it. The Start menu's apps
  never hold the engine: suggestions are empty until the first read is done, then the window is
  told to ask again. Other window arrangers are looked for at most every five minutes, not after
  every switch. Previews no longer keep the monitors they read, which could hide a display change
  from the refit that follows it.
- Changes Mosaïk makes to the workspaces itself (a tab clicked, a window dropped) are written on a
  thread of their own once changes stop for 300 ms, at least every 2 s while they go on, and at
  once when Mosaïk quits (`mosaik_core::saver`): the click no longer waits for the disk. A save the
  user asked for still waits for it, and nothing older is written after it.
- How small a window goes, which its app answers (up to 100 ms when it is busy), is asked again
  only after 10 s: Home's picture asked every window each time it was drawn.

**Files**
- Settings edited by hand out of range are brought into range on load, with one note saying
  what changed; a damaged workspace is described in words ("an app's zone is gone").

## Consequences

- The UI tour finds Automatic, Undo swaps, More settings and Back to my screen; the probes check a
  press as the side menu opens (`smart`: nothing moves, the menu stays) and measure both scans
  (`classify`: the engine's and every window's).
- Opening a workspace can be undone as Arrange can (the same `undo_arrange`, for ten minutes,
  until a workspace is saved or edited): every window it moved or minimised goes back, the ones it
  brought back from the taskbar to it, and the workspace open before is open again. Home offers
  Undo after Open; a set started is not undone. The workspace probe checks that the window it
  minimised comes back where it was.
- Not done here: Arrange still keeps its zones in a workspace (the open one, or a new one), as
  ADR 0014 decided; it now says so (since ADR 0030 it keeps them in none, and says only what it
  did). Windows' own text size is not applied to the window (Ctrl and + is); making tabs from a
  workspace's page is next.

## Amended by ADR 0030 (2026-10-15)

Arrange changes no workspace: it says what moved and what was minimised, never which workspace
opens this way.

## Amended by ADR 0031 (2026-10-15)

The picture says "Nothing moves yet." beside Arrange windows (Mosaïk's tiles, not the spark),
and "Drag a window onto another to swap them, or a line between zones to resize them."; Undo
swaps is Undo changes. The layouts are all in view (Automatic, then eight in two rows, without
Top + bottom and Three rows), with no More layouts. The first run's line of welcome is gone.

## Amended by ADR 0032 (2026-10-16)

The main button says Move windows (with Mosaïk's tiles); "Nothing moves yet." stays beside it.
