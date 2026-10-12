# 0006. A separate watchdog, a journal and a marker on every hidden window

- Status: accepted
- Date: 2026-10-11

## Context

Mosaïk hides other apps' windows (background tabs). If Mosaïk crashes, is killed or is updated
while windows are hidden, those windows must come back; "never lose the user's windows" is a
promise.

## Decision

- Before hiding a window, Mosaïk records it in `hidden-windows.json` (atomic write, last-good
  backup) and sets a `Mosaik.Hidden` property on the window.
- At startup Mosaïk starts `mosaik-watchdog --watch <pid> --journal <path>`, a separate,
  windowless process of about 250 KB that waits for Mosaïk to end, however it ends. If anything
  is left in the journal, or any window still carries the marker, it restores it, keeps in the
  journal only what it could not restore, and says so in a message box with the next step.
- The same restore runs from the tray ("Rescue windows"), from `mosaikctl rescue` and from
  `mosaik --rescue`, which both work even when Mosaïk is not running or not answering.
  `mosaik --safe-mode` starts Mosaïk without moving or hiding anything.
- When the main process ends with anything but a normal exit, the watchdog says so in a message
  box once the windows are back, with what to do next (a crash, or Mosaïk ended from Task
  Manager). A journal it cannot read (damaged, or from a newer Mosaïk) is left untouched; the
  markers still find the windows.
- The watchdog logs its steps with timings (never window titles) to `watchdog.log` next to the
  journal.

## Measurements

`mosaik-probe watchdog` (2026-10-11): a stand-in main process hid four windows (shell cloak,
minimise, park, and a shell-cloaked Notepad) and was killed. With the watchdog started by the
stand-in itself, as Mosaïk does, all four were back 18 ms after the kill, in place (0 px), with
the journal emptied; started by another process, 59 ms.

## Consequences

- One more executable to sign and ship. It has no window, no network access and no settings.
- A first version of the probe killed the stand-in before it had started its watchdog; the
  journal and markers cover that window of time too, through the rescue paths above.
