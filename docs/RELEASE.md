# Releasing Mosaïk

Everything up to the package is automated, and sending it is one line; publishing stays the
owner's (the Store submission, the GitHub push, the site deploy). Version 2.0.0 is set in
`Cargo.toml` and `apps/desktop/package.json`; `CHANGELOG.md` says what is in it, and
`installer/msix/whats-new.txt` is what the Store shows as "What's new in this version".

## 1. Build, test and package (on Windows)

```powershell
.\mosaik.ps1 build      # interface + release binaries (about 3 minutes the first time)
.\mosaik.ps1 test       # every test, clippy and the interface checks
.\mosaik.ps1 package    # target\msix\Mosaik_2.0.0.0_x64.msix
```

A Mosaïk running from the same folder quits for the build (every window it hid comes back) and
starts again after.

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

## 3. Ship it: the site and the app in one line

From the folder with `mosaik1.sh` and the repository (`mosaik/`) beside it, in Git Bash:

```bash
./mosaik1.sh --deploy --app
```

`--app` runs the app's `.\mosaik.ps1 publish` first: the tests, the build and the package, then,
after you say yes, the package goes to the Microsoft Store (ADR 0025) with "What's new" from
`installer/msix/whats-new.txt`; nothing else of the listing changes. If the app is not sent, the
site is not deployed either. Then `--deploy` puts the site online. The Store checks the new
version, usually within a day or two, and then updates everyone. `./mosaik1.sh --app` sends the
app alone; `.\mosaik.ps1 publish` does the same from PowerShell.

### Sending from a script: set up once

The Store's submission API acts for your Partner Center account through a Microsoft Entra
application:

1. Partner Center, **Settings** (gear), **Account settings**, **Tenants**: if the account has no
   Microsoft Entra tenant yet, create one (free) and sign in to it once.
2. **Account settings**, **User management**, **Microsoft Entra applications**: **Create Microsoft
   Entra application** (or add an existing one), with the **Manager** role.
3. Select it: note its **Tenant ID** and **Client ID**, then **Add new key** and copy the **Key**
   (it is shown once).
4. The first `--app` (or `.\mosaik.ps1 publish`) asks for the three and keeps them for your
   Windows user only, encrypted (`%APPDATA%\Mosaik\store-publishing.xml`; delete it to give them
   again). Keys expire: Partner Center shows when, and makes a new one the same way.

### By hand, in Partner Center

Apps and games, Mosaïk (Store ID `9PHR0L7S1QVD`), Start submission. 1.0.0 was published on
2026-10-12; Partner Center copies the last submission, so only the package and "What's new"
change. Upload `target\msix\Mosaik_2.0.0.0_x64.msix` (and remove the older package), paste
`installer/msix/whats-new.txt` into "What's new in this version", and submit.

The rest of the submission, for reference (already set by submission 1):

- **Pricing and availability**: free, all markets.
- **Properties**: category Productivity; privacy policy `https://xn--mosak-fta.com/conditions`;
  website `https://xn--mosak-fta.com`; support `https://github.com/gimzdev/mosaik/issues`.
- **Age ratings**: the questionnaire (no user-generated content, no purchases, no data shared).
- **Packages**: the `runFullTrust` capability, when asked why:
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
  - *Screenshots*: at least one, 1920x1080, of Mosaïk in use (Home with a layout chosen, zones
    with tags, a tab strip, the switcher, the side menu).
  - *Search terms*: window manager, workspaces, tabs, zones, tiling.
- Submit by 2027-01-11 or the reserved name is released (done: 1.0.0 is published).

## 4. After the Store has the new version

- The Download buttons install from the Store's web installer; check one on a clean account.
- The site: `./mosaik1.sh --deploy` (already done by the line in section 3).
- GitHub: from the folder above the repositories, `./deploy.sh push mosaik` force-pushes this
  repository to `master` as it is (its README included); the full `./deploy.sh` does every
  repository, and leaves Mosaïk's README as it is written here. Then, if wanted, a release
  tagged with the version (`v2.0.0`) with that version's section of `CHANGELOG.md` (no files to
  attach: Mosaïk installs from the Store).
- `winget install 9PHR0L7S1QVD` works through winget's Store source once the listing is live.
