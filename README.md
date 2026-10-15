<div align="center">

<h1>Mosaïk</h1>

<h3>Every window, in its place.</h3>

[![Windows 11 and 10](https://img.shields.io/badge/Windows_11_and_10-2B44D9?style=for-the-badge)](https://xn--mosak-fta.com/download)
[![Rust](https://img.shields.io/badge/Rust-161B25?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Tauri](https://img.shields.io/badge/Tauri_2-12786A?style=for-the-badge&logo=tauri&logoColor=white)](https://tauri.app)
[![CC0](https://img.shields.io/badge/Public_domain-CC0-F4B13A?style=for-the-badge)](LICENSE)

<hr>

<h2>About</h2>

<p>Mosaïk gives every project its own workspace:<br>
your apps and windows arranged and ready in one click.</p>

<p>Free forever, with no account, no telemetry and no cloud.<br>
Your layouts stay on your computer.</p>

[![Download for Windows](https://img.shields.io/badge/Download_for_Windows-2B44D9?style=for-the-badge)](https://xn--mosak-fta.com/download)

<hr>

<h2>What it does</h2>

<table align="center">
<tr>
<td align="center" width="33%">

<h3>Zones</h3>

Numbered, coloured areas<br>
that belong to the project<br>
Drag a window near one<br>
and it is pulled in;<br>
resize it and they follow

</td>
<td align="center" width="33%">

<h3>Tabs</h3>

Tabs for any app,<br>
not only the browser<br>
Drop a window on a tag<br>
Pull a tab out to make it<br>
a window again

</td>
<td align="center" width="33%">

<h3>Workspaces</h3>

See where windows will go,<br>
then arrange them in one click<br>
Switch from the tray, the<br>
screen's edge or the keyboard<br>
Positions, sizes and tabs<br>
return together

</td>
</tr>
<tr>
<td align="center" width="33%">

<h3>Every monitor</h3>

Zones are kept as proportions<br>
and fit each monitor again,<br>
whatever its scaling

</td>
<td align="center" width="33%">

<h3>On your computer</h3>

No network requests at all<br>
Updates come through<br>
the Microsoft Store

</td>
<td align="center" width="33%">

<h3>Safe</h3>

A watchdog brings back<br>
every hidden window<br>
if Mosaïk stops

</td>
</tr>
</table>

<hr>

<h2>Using it</h2>

<p>Open Mosaïk from the Start menu or its tray icon: <b>Your screen</b> shows your windows as <b>Automatic</b> would lay them out,<br>
each in its zone. Choose a layout instead to see it first, and drag a window onto another to swap them.<br>
Nothing moves until <b>Arrange</b>; <b>Undo</b> puts them back.</p>

<p>Beside it, click a workspace and the picture shows it as it will open; <b>Open</b> opens it.<br>
<b>Suggested</b> offers sets of your apps that go together, such as <b>Coding</b>:<br>
<b>Start these apps</b> opens them side by side, three to a view, and starts the ones that are not open.</p>

<p>Arrange the apps of one project, press <kbd>Win</kbd> <kbd>Alt</kbd> <kbd>Space</kbd>, then <kbd>S</kbd> and a name.<br>
Each place becomes a zone with its own colour, numbered 1, 2, 3 from the top left.</p>

<p>Press <kbd>Win</kbd> <kbd>Alt</kbd> <kbd>Space</kbd> again to switch: its windows come back and missing apps start.</p>

<table align="center">
<tr><td><kbd>1</kbd> … <kbd>9</kbd>, then <kbd>F1</kbd> …</td><td>open a workspace</td></tr>
<tr><td><kbd>↑</kbd> <kbd>↓</kbd> <kbd>Enter</kbd></td><td>pick and open</td></tr>
<tr><td>typing</td><td>filters by name</td></tr>
<tr><td><kbd>S</kbd></td><td>saves the windows on screen</td></tr>
<tr><td><kbd>Esc</kbd></td><td>closes the switcher</td></tr>
</table>

<p>Or point at the thin line at the right edge of the screen: the side menu has <b>Arrange</b>, five layouts and your workspaces.<br>
Arrange lays your screen out for what you are doing, the window you work in the largest.</p>

<p>Resize a window in a zone and the zones beside it follow.<br>
While dragging or resizing a window, hold <kbd>Shift</kbd> to move it freely.<br>
Workspaces, their apps and zones, shortcuts and everything else are in Mosaïk's window, from the tray icon.</p>

<hr>

<h2>Two files</h2>

<p>Everything travels in two files, each runnable on its own on another machine.</p>

<table align="center">
<tr>
<td align="center" width="50%">

<h3><code>mosaik.ps1</code></h3>

The app: its whole source inside,<br>
with the steps to check, build,<br>
test, run and package it.<br>
Right-click, <b>Run with PowerShell</b>.

</td>
<td align="center" width="50%">

<h3><code>mosaik1.sh</code></h3>

The website at mosaïk.com:<br>
writes it, installs it, runs it<br>
or deploys it.<br>
<code>bash mosaik1.sh</code>

</td>
</tr>
</table>

```powershell
.\mosaik.ps1 check      # what building needs (Rust, C++ build tools, Node.js 22.13+, WebView2)
.\mosaik.ps1 build      # the interface, then mosaik.exe, its watchdog and mosaikctl
.\mosaik.ps1 test       # every test and check
.\mosaik.ps1 run        # start the Mosaïk just built
.\mosaik.ps1 package    # the Store package (MSIX)
.\mosaik.ps1 publish    # tests, builds and packages it, then sends it to the Microsoft Store
.\mosaik.ps1 pack       # a one-file copy with this source inside (target\mosaik.ps1)
```

<hr>

<h2>Inside</h2>

<table align="center">
<tr><td><code>crates/mosaik-core</code></td><td>zones, layouts, matching, tabs, settings: pure logic, tested on any OS</td></tr>
<tr><td><code>crates/mosaik-win</code></td><td>windows, monitors, DPI, hiding, input: everything that talks to Windows</td></tr>
<tr><td><code>crates/mosaik-overlay</code></td><td>zone frames, tags, tab strips, the side menu and toasts, drawn natively</td></tr>
<tr><td><code>crates/mosaik-engine</code></td><td>the engine thread: events, workspaces, the switcher, shortcuts</td></tr>
<tr><td><code>crates/mosaik-watchdog</code></td><td>brings hidden windows back if Mosaïk stops</td></tr>
<tr><td><code>crates/mosaik-ipc</code></td><td>the named-pipe protocol and <code>mosaikctl</code></td></tr>
<tr><td><code>crates/mosaik-probe</code></td><td>the probes behind the measurements in <a href="docs/STATUS.md">status</a>, run on a real desktop</td></tr>
<tr><td><code>apps/desktop</code></td><td>the Tauri shell and Mosaïk's window (React, TypeScript)</td></tr>
<tr><td><code>installer/</code></td><td>the Store package and the third-party notices</td></tr>
<tr><td><code>docs/</code></td><td><a href="docs/architecture.md">architecture</a>, <a href="docs/STATUS.md">status and measurements</a>, <a href="docs/adr">decisions</a></td></tr>
</table>

<hr>

<h2>Licence</h2>

<p>Mosaïk is dedicated to the public domain under <a href="LICENSE">CC0 1.0</a>: take it, change it, ship it.<br>
The open-source software it is built with keeps its own licences, listed in <a href="THIRD-PARTY-NOTICES.txt">THIRD-PARTY-NOTICES.txt</a>.</p>

<p><a href="https://xn--mosak-fta.com/conditions">Privacy and terms</a> · <a href="https://www.patreon.com/gimzdev">Support on Patreon</a></p>

</div>
