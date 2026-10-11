# Architecture

## Principles

The owner's first rule, above every other trade-off: **minimal, efficient, elegant**.

- **One click to install.** The download button on mosaïk.com fetches one small signed installer
  directly. It installs for the current account only: no administrator prompt, no extra runtimes
  (the C runtime is linked into the binaries; WebView2 ships with Windows 11), no reboot. Mosaïk
  starts when the installer finishes.
- **Easy to use.** One global shortcut, a tray icon, a short first-run welcome. Few settings, each
  with a sensible default.
- **Light.** Measured idle CPU and memory budgets (`docs/STATUS.md`), and nothing running that the
  user did not ask for.
- **Trustworthy.** No telemetry, accounts or cloud; the only network call is the opt-in update
  check. A crash never strands a window. Every failure is explained with a way forward.

## Processes

```
mosaik.exe            engine thread (windows, zones, workspaces, tabs, hotkey, overlays)
  ├─ pipe threads     mosaikctl and second launches (ADR 0007)
  ├─ main thread      Tauri: tray icon, switcher and settings windows (WebView2)
  └─ starts ──────────► mosaik-watchdog.exe   restores hidden windows if mosaik.exe dies (ADR 0006)
mosaikctl.exe         command line client of the pipe
```

The engine thread owns every change Mosaïk makes to other apps' windows, so changes happen in
order on one thread with a Win32 message loop (hotkeys, window events, timers). The interface
and `mosaikctl` send it the same commands (`mosaik_ipc::protocol`); the interface in process,
`mosaikctl` over the pipe.

## Crates

| Crate | What it is |
| --- | --- |
| `mosaik-core` | Pure logic, no Win32: geometry, topologies, zones, layouts, matching, classification, snapping, placement maths, tabs, the journal, settings and the store. Tested on every platform. |
| `mosaik-win` | The Win32 adapter: enumerating and classifying windows, placing them by their visible frame, hiding them, monitors and DPI, hooks, hotkeys, processes, rescue. Decides nothing. |
| `mosaik-overlay` | Native layered windows for the latency-critical overlays: zone previews, frames, tab strips. |
| `mosaik-engine` | The engine thread, its commands, the pipe server and the watchdog launcher. |
| `mosaik-ipc` | The versioned protocol, the named pipe and `mosaikctl`. |
| `mosaik-watchdog` | The watchdog process. |
| `mosaik-probe` | The measurements behind the ADRs, re-run before every release. Not shipped. |
| `apps/desktop` | `mosaik.exe`: Tauri shell and the React interface. |

The brief's layout lists five crates; `mosaik-engine` is added so the engine can be tested and
measured without the interface, and the Tauri crate stays a thin shell.
