# Releasing Mosaïk

Everything up to the package is automated; publishing is the owner's (Store submission, the
GitHub push and release, the site deploy). Version 1.1.0 is set in `Cargo.toml` and
`apps/desktop/package.json`; `CHANGELOG.md` says what is in it.

## 1. Build, test and package (on Windows)

```powershell
.\mosaik.ps1 build      # interface + release binaries (about 3 minutes the first time)
.\mosaik.ps1 test       # every test, clippy and the interface checks
.\mosaik.ps1 package    # target\msix\Mosaik_1.1.0.0_x64.msix
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

1.0.0 was submitted on 2026-10-12 and is in certification; a submission in certification cannot
be changed. 1.1.0 goes as the next submission, once 1.0.0 is published (or after cancelling the
one in certification): Partner Center copies the last submission, so only the package, "What's
new in this version" and the listing below change.

- **Pricing and availability**: free, all markets.
- **Properties**: category Productivity; privacy policy `https://xn--mosak-fta.com/conditions`;
  website `https://xn--mosak-fta.com`; support `https://github.com/gimzdev/mosaik/issues`.
- **Age ratings**: the questionnaire (no user-generated content, no purchases, no data shared).
- **Packages**: upload `Mosaik_1.1.0.0_x64.msix` (and remove the older package). The
  `runFullTrust` capability, when asked why:
  "Mosaïk is a desktop app that arranges the windows of the user's other apps (moves, resizes,
  groups them into tabs) and registers one global keyboard shortcut. It needs full trust to see
  and move those windows. It makes no network requests."
- **Store listing** (English):
  - *Description*: "Mosaïk gives every project its own workspace: your apps and windows arranged
    and ready in one click. Point at a layout and see where your windows would go; click, and
    they go. Carve the screen into named, coloured zones, or press Arrange and every window goes
    where it suits; drag a window near a zone and it is pulled in, resize it
    and the zones beside it make room. Stack any windows into tabs, browser tabs included, and
    flip between them. Save a workspace once and bring it back from the tray, the line at the
    screen's edge or Win+Alt+Space: positions, sizes and tabs return together, and missing apps
    start. Free and open source. No account, no telemetry, no network requests: your layouts
    stay on your computer."
  - *Features*: Zones that belong to the project, not the monitor · Arrange: every window in its
    place in one click, with undo · Ten layouts, previewed on your own windows before you pick
    one · Tabs for any app, not only the browser · One shortcut to
    switch workspaces, with a preview of each · Works across monitors and scales · Your layouts
    stay on your computer · A safety net brings back every window, always.
  - *What's new in this version* (1.1.0): "A home page: point at Arrange or one of ten layouts
    and see your windows glide to where they would go; click, and they do, with undo. Every
    window starts at the top of its zone, and zones sit closer together. Resize a window and the
    zones beside it follow. A quick menu at the screen's edge. Four more zone presets."
  - *Screenshots*: at least one, 1920x1080, of Mosaïk in use (the home page with a layout
    pointed at, zones with tags, a tab strip, the switcher).
  - *Search terms*: window manager, workspaces, tabs, zones, tiling.
- **Submission options**: publish as soon as it passes certification (a day or two), or by hand.

## 4. After the Store listing is live

- The Download buttons install from the Store's web installer; check one on a clean account.
- Deploy the site: `./mosaik1.sh --deploy`.
- Push and release on GitHub: push `v1-build` to `main`, tag the version (`v1.0.0`, then
  `v1.1.0`), and create the release with that version's section of `CHANGELOG.md` (no files to
  attach: Mosaïk installs from the Store).
- `winget install 9PHR0L7S1QVD` works through winget's Store source once the listing is live.
