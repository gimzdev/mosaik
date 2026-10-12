# Architecture

## Principles

The owner's first rule, above every other trade-off: **minimal, efficient, elegant**.

- **One click to install.** The Download button on mosaïk.com fetches a small installer signed
  by Microsoft, right in the browser (the Store's direct web installer, ADR 0008). It installs for
  the current account only: no administrator prompt, no extra runtimes (the C runtime is linked
  into the binaries; WebView2 ships with Windows 11), no reboot. Mosaïk starts when it is done,
  and Windows keeps it up to date.
- **Easy to use.** One global shortcut, a tray icon, a short first-run welcome. Few settings, each
  with a sensible default.
- **Light.** Measured idle CPU and memory budgets (`docs/STATUS.md`), and nothing running that the
  user did not ask for.
- **Trustworthy.** No telemetry, accounts or cloud; Mosaïk makes no network request at all
  (updates come through the Store). A crash never strands a window. Every failure is explained
  with a way forward.

## Processes

```
mosaik.exe            engine thread: windows, zones, workspaces, tabs, hotkey, the switcher and
  │                   the overlays (native, ADR 0002)
  ├─ mover threads    one per window being moved, so a hung app never holds up the engine
  ├─ pipe threads     mosaikctl and second launches (ADR 0007)
  ├─ main thread      Tauri: the tray icon, and web windows (settings, editor, welcome) created
  │                   when opened and destroyed when closed
  └─ starts ──────────► mosaik-watchdog.exe   restores hidden windows if mosaik.exe dies (ADR 0006)
mosaikctl.exe         command line client of the pipe
```

The engine thread owns every change Mosaïk makes to other apps' windows, so changes happen in
order on one thread with a Win32 message loop (hotkeys, window events, timers). The interface
and `mosaikctl` send it the same commands (`mosaik_ipc::protocol`); the interface in process,
`mosaikctl` over the pipe. Moving a window is a synchronous call into its app, so the engine
hands each move to a short-lived thread and keeps handling its messages while it waits, at most
2.5 s (`mover.rs`).

## How workspaces behave

The decisions are pure functions in `mosaik_core::workspace`, tested on every monitor fixture;
the engine gathers windows and applies the result.

- **Saving** (S in the switcher, then a name) turns the windows on screen into a workspace.
  Windows that tile, side by side without overlapping, become zones named after their app;
  windows stacked exactly on top of each other share a zone; maximised windows and windows
  floating over others keep their own place. Windows in zones then move inside them, below the
  band that carries the zone's tag. Saving under an existing name updates that workspace and
  keeps the zones its windows sit in.
- **Being in a zone** has one rule everywhere (`holds`): the window's centre is in the zone and
  the window and the zone's window area cover at least 70% of each other. A browser that will
  not shrink to a narrow zone still counts; a small window floating over a zone does not.
- **A layout changes only by the user's hand**: saving, dropping a window into a zone, or moving
  one of the workspace's windows by mouse. Windows an app moved on its own, or a window
  maximised for a moment, go back to their places the next time the workspace opens.
- **Opening** a workspace finds its windows (program, then window class and title to tell
  several windows of one app apart), moves them all at once, starts missing apps, minimises the
  previous workspace's other windows that are on screen, brings its windows forward and gives
  the front one the keyboard. 400 ms later, windows an app moved back are put in place once.
- **Frames**: every window in a zone of the open workspace wears a ring in the zone's colour and
  a tag with its name, drawn right above the window in the stacking order, so windows that
  cover it cover its frame too. Frames follow moves through window events, hooked per process.
- **Drag to snap**: while a window is dragged by its title bar, the open workspace's zones show
  under it; the zone under the pointer (or within 48 px) is tinted, and the window is pulled into
  it on release. Shift moves freely; a setting swaps that. A window that was not part of the
  workspace joins it.
- **Displays changing** (a monitor plugged in or out, a scale or taskbar change, waking up) fits
  the open workspace's windows to the new arrangement: zones are fractions of a monitor's work
  area, and each remembered monitor is matched to one connected now.

## Crates

| Crate | What it is |
| --- | --- |
| `mosaik-core` | Pure logic, no Win32: geometry, topologies, zones, layouts, matching, classification, snapping, placement maths, tabs, the journal, settings and the store. Tested on every platform. |
| `mosaik-win` | The Win32 adapter: enumerating and classifying windows, placing them by their visible frame, hiding them, monitors and DPI, hooks, hotkeys, processes, rescue. Decides nothing. |
| `mosaik-overlay` | Native overlays drawn with Direct2D in software: rings as four thin windows, name tags, the drop-target tint, tab strips. |
| `mosaik-engine` | The engine thread, its commands, the pipe server and the watchdog launcher. |
| `mosaik-ipc` | The versioned protocol, the named pipe and `mosaikctl`. |
| `mosaik-watchdog` | The watchdog process. |
| `mosaik-probe` | The measurements behind the ADRs, re-run before every release. Not shipped. |
| `apps/desktop` | `mosaik.exe`: Tauri shell and the React interface. |

The brief's layout lists five crates; `mosaik-engine` is added so the engine can be tested and
measured without the interface, and the Tauri crate stays a thin shell.
