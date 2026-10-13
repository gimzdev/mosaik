# Releasing Mosaïk

Everything up to the package is automated; publishing is the owner's (Store submission, the
GitHub push and release, the site deploy). Version 2.0.0 is set in `Cargo.toml` and
`apps/desktop/package.json`; `CHANGELOG.md` says what is in it.

## 1. Build, test and package (on Windows)

```powershell
.\mosaik.ps1 build      # interface + release binaries (about 3 minutes the first time)
.\mosaik.ps1 test       # every test, clippy and the interface checks
.\mosaik.ps1 package    # target\msix\Mosaik_2.0.0.0_x64.msix
```

## 2. Check by hand on real machines

The probes cover one 1920x1080 monitor at 100% (`docs/STATUS.md`). Before the first release,
on Windows 11 and, if one is at hand, Windows 10 22H2:

- Two monitors with different scales (the virtual displays already installed, at 100% and
  150%): save a workspace across both, switch away and back, unplug one (disable the adapter in
  Device Manager) and plug it back.
- 125%, 150% and 200% on the main monitor: frames, tags and strips sharp and in place.
- Sleep and wake; sign out and in; a Remote Desktop session into the PC (needs Windows Pro).
- Windows' "Snap windows" off and on.
- A contrast theme (Settings, Accessibility, Contrast themes): Mosaïk's window, the switcher,
  tags and tab strips readable; then back to none. (Light and dark are checked by
  `mosaik-probe ui-tour`; contrast themes are a Windows setting the probes do not change.)
- Narrator on Mosaïk's window: each page's controls are read with their names (the probe lists
  what UI Automation gives Narrator; this is the check by ear).

## 3. Submit to the Microsoft Store (Partner Center)

Apps and games, Mosaïk (Store ID `9PHR0L7S1QVD`), Start submission. Submit by 2027-01-11 or the
reserved name is released.

1.0.0 was published on 2026-10-12. 2.0.0 is the next submission (1.1 was never submitted):
Partner Center copies the last submission, so only the package, "What's new in this version"
and the listing below change.

- **Pricing and availability**: free, all markets.
- **Properties**: category Productivity; privacy policy `https://xn--mosak-fta.com/conditions`;
  website `https://xn--mosak-fta.com`; support `https://github.com/gimzdev/mosaik/issues`.
- **Age ratings**: the questionnaire (no user-generated content, no purchases, no data shared).
- **Packages**: upload `Mosaik_2.0.0.0_x64.msix` (and remove the older package). The
  `runFullTrust` capability, when asked why:
  "Mosaïk is a desktop app that arranges the windows of the user's other apps (moves, resizes,
  groups them into tabs) and registers one global keyboard shortcut. It needs full trust to see
  and move those windows. It makes no network requests."
- **Store listing** (English):
  - *Description*: "Mosaïk gives every project its own workspace: your apps and windows arranged
    and ready in one click. See your screen as it will be before anything moves: Arrange lays
    it out for what you are doing, or pick one of ten layouts. Carve the screen into named,
    coloured zones; drag a window near a zone and it is pulled in, resize it and the zones
    beside it make room. Stack any windows into tabs, browser tabs included, and flip between
    them. Save a workspace once and bring it back from the tray, the side menu at the screen's
    edge or Win+Alt+Space: positions, sizes and tabs return together, and missing apps start.
    Free and open source. No account, no telemetry, no network requests: your layouts stay on
    your computer."
  - *Features*: Zones that belong to the project, not the monitor · Arrange: every window in its
    place in one click, with undo · Ten layouts, shown on your own windows before anything
    moves · Sets of the apps you use together, opened in one click · Tabs for any app, not only
    the browser · One shortcut to switch workspaces · Works across monitors and scales · Your
    layouts stay on your computer · A safety net brings back every window, always.
  - *What's new in this version* (2.0.0): "Mosaïk 2: a new window, dark by default. See your
    screen as it will be before anything moves: Smart lays it out for what you are doing, or
    pick one of ten layouts, then Arrange (with undo). Your workspaces beside it, your apps by
    their own icons, no more pop-ups. Zones make room for a window that needs more, windows meet
    edge to edge, and the side menu at the screen's edge has Arrange, layouts and workspaces."
  - *Screenshots*: at least one, 1920x1080, of Mosaïk in use (Home with a layout chosen, zones
    with tags, a tab strip, the switcher, the side menu).
  - *Search terms*: window manager, workspaces, tabs, zones, tiling.
- **Submission options**: publish as soon as it passes certification (a day or two), or by hand.

## 4. After the Store listing is live

- The Download buttons install from the Store's web installer; check one on a clean account.
- Deploy the site: `./mosaik1.sh --deploy`.
- Push and release on GitHub: push `v1-build` to `main`, tag the version (`v1.0.0`, then
  `v2.0.0`), and create the release with that version's section of `CHANGELOG.md` (no files to
  attach: Mosaïk installs from the Store).
- `winget install 9PHR0L7S1QVD` works through winget's Store source once the listing is live.
