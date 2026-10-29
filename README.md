<div align="center">

# Mosaïk

Window manager for Windows 11 and 10: zones, tabs for any app, workspaces in one click.

<a href="https://apps.microsoft.com/detail/9PHR0L7S1QVD"><img src="https://get.microsoft.com/images/en-us%20dark.svg" width="200" alt="Get it from Microsoft"></a>

[mosaïk.com](https://mosaïk.com) · Rust · Tauri · React · TypeScript

<img src="docs/screenshots/mosaik-preview.gif" alt="Mosaïk: three messy windows, the side menu at the edge of the screen, Half + stack, then Thirds">

<table>
<tr>
<td align="center" width="50%"><img src="docs/screenshots/mosaik-site.png" alt="mosaïk.com"><br><sub>mosaïk.com</sub></td>
<td align="center" width="50%"><img src="docs/screenshots/mosaik-app-home.png" alt="Home"><br><sub>Home</sub></td>
</tr>
<tr>
<td align="center" width="50%"><img src="docs/screenshots/mosaik-app-workspace.png" alt="A workspace"><br><sub>A workspace</sub></td>
<td align="center" width="50%"><img src="docs/screenshots/mosaik-app-zones.png" alt="Zones"><br><sub>Zones</sub></td>
</tr>
</table>

<hr>

Mosaïk gives every project its own workspace. Windows live in numbered, coloured zones that belong to the
project rather than the monitor, apps can share a tab strip, and one click brings back the positions,
sizes and tabs of a whole workspace, starting any app that is missing. It is free, makes no network
requests and keeps everything on your computer.

| | |
|:---|:---|
| **Zones** | Numbered, coloured areas that belong to the project; drag a window near one and it is pulled in |
| **Tabs for any app** | Drop a window on another's tag and they share a tab strip; pull one out and it is a window again |
| **Workspaces** | See where every window will go, then open it in one click; missing apps start |
| **Every monitor** | Zones are proportions, so they fit each monitor and scaling |

A watchdog process restores every hidden window if Mosaïk ever stops, and apps that cannot be resized
smaller than a zone are left alone and explained, never forced.

<sub>`mosaik.ps1` is the app in one file, its source inside; `mosaik.sh` writes and deploys the site. Code: `crates/` (logic, Windows, overlays, engine, watchdog, protocol), `apps/desktop` (the window), `docs/` ([architecture](docs/architecture.md), [status](docs/STATUS.md), [decisions](docs/adr)).</sub>

<sub>Public domain under [CC0 1.0](LICENSE); the software it is built with keeps its own licences ([notices](THIRD-PARTY-NOTICES.txt)). [Privacy and terms](https://xn--mosak-fta.com/conditions) · [Support on Patreon](https://www.patreon.com/gimzdev)</sub>

</div>
