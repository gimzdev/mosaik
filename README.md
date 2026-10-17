<div align="center">

# Mosaïk

Every project its own workspace on Windows: your apps and windows arranged in one click.

[mosaïk.com](https://xn--mosak-fta.com) · [Microsoft Store](https://apps.microsoft.com/detail/9PHR0L7S1QVD) · Rust · Tauri · React · TypeScript

</div>

<table>
<tr>
<td align="center" width="50%"><img src="docs/screenshots/mosaik-site.png" alt="mosaïk.com"><br><sub>mosaïk.com</sub></td>
<td align="center" width="50%"><img src="docs/screenshots/mosaik-app-home.png" alt="Home: your windows in numbered, coloured zones, and your workspaces"><br><sub>Home</sub></td>
</tr>
<tr>
<td align="center" width="50%"><img src="docs/screenshots/mosaik-app-workspace.png" alt="A workspace: where its apps go"><br><sub>A workspace</sub></td>
<td align="center" width="50%"><img src="docs/screenshots/mosaik-app-zones.png" alt="The zone editor"><br><sub>Zones</sub></td>
</tr>
</table>

## What it does

- **Zones**: numbered, coloured areas that belong to the project. Drag a window near one and it is pulled in; resize it and the zones beside it follow.
- **Tabs for any app**: drop a window on another's tag and they share a tab strip; pull a tab out and it is a window again.
- **Workspaces**: see where every window will go, then open the workspace in one click. Positions, sizes and tabs come back together, and missing apps start.
- **Every monitor**: zones are kept as proportions, so they fit each monitor again, whatever its scaling.
- **Yours**: free, no account, no telemetry, no network at all; updates come through the Microsoft Store. A watchdog brings back every hidden window if Mosaïk stops.

## Using it

Open Mosaïk from the Start menu or the tray. **Your screen** shows your windows as a layout would place them: drag a window into another zone, onto another window to swap them, or a line to resize the zones. Nothing moves until **Move windows**, and **Undo** puts them back. Click a workspace beside it to see it, then **Open**.

<kbd>Win</kbd> <kbd>Alt</kbd> <kbd>Space</kbd> opens the switcher: <kbd>1</kbd> to <kbd>9</kbd> opens a workspace, typing filters, <kbd>S</kbd> saves the windows on screen as one. Hold <kbd>Shift</kbd> while dragging a window to move it freely.

## Build it

`mosaik.ps1` is the app in one file, its source inside; `mosaik.sh` writes and deploys the site.

```powershell
.\mosaik.ps1 check    # what building needs: Rust, C++ build tools, Node.js 22.13+, WebView2
.\mosaik.ps1 build    # the interface, then mosaik.exe, its watchdog and mosaikctl
.\mosaik.ps1 test     # every test and check
.\mosaik.ps1 run      # start the Mosaïk just built
```

A one-file copy works in the `mosaik` folder beside it, whatever its version, and in Mosaïk's repository as it is when that is the folder. The code: `crates/` (the logic, Windows, overlays, engine, watchdog, protocol), `apps/desktop` (the window), `docs/` ([architecture](docs/architecture.md), [status](docs/STATUS.md), [decisions](docs/adr)).

## Licence

Public domain under [CC0 1.0](LICENSE). The software it is built with keeps its own licences ([THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt)).

[Privacy and terms](https://xn--mosak-fta.com/conditions) · [Support on Patreon](https://www.patreon.com/gimzdev)
