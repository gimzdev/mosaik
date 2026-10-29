# Releasing Mosaïk

Everything up to the package is automated, and sending it is one line; publishing stays the
owner's (the Store submission, the GitHub push, the site deploy). Version 1.0.4 is set in
`Cargo.toml` and `apps/desktop/package.json`; `CHANGELOG.md` says what is in it, and
`installer/msix/whats-new.txt` is what the Store shows as "What's new in this version". The
version stays 1.0 (the owner): each release takes the next third number (1.0.2, 1.0.3...), as the
Store takes only a higher version than the one it has. A version it has had once is never taken
again, even built again from the same source: a listing sent after its version is out goes with
the next number (1.0.4 is 1.0.3 with the listing's new screenshots).

## 1. Build, test and package (on Windows)

```powershell
.\mosaik.ps1 build      # interface + release binaries (about 3 minutes the first time)
.\mosaik.ps1 test       # every test, clippy and the interface checks
.\mosaik.ps1 package    # target\msix\Mosaik_1.0.4.0_x64.msix
```

A Mosaïk running from the same folder quits for the build (every window it hid comes back) and
starts again after. Smart App Control can refuse a new build's files for a while: when it refuses
`mosaikctl.exe`, which asks Mosaïk to quit, Mosaïk is stopped instead (its watchdog first) and
the watchdog's quiet rescue brings back its windows; when it refuses the new `mosaik.exe`, the
build says so and `.\mosaik.ps1 run` starts it later. The files it refuses while building are made
again until it lets them through, six runs at most.

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

From the folder with `mosaik.sh` and the repository (`mosaik/`) beside it, in Git Bash:

```bash
./mosaik.sh --deploy --app
```

It asks the Store first (`publish.ps1 -Check`): while the Store still checks the version sent
before, or when it has this version already or a newer one, it says so and nothing is tested,
built or sent. Otherwise it tests the code (only when the tests have not passed on it yet here:
a new version number, the changelog, docs, "What's new" and the listing do not count, and
`.\mosaik.ps1 test` always runs them), builds and packages the app and sends it to the Microsoft
Store (ADR 0026): a new submission copied from the last one, this package in place of the old
one, "What's new" from `installer/msix/whats-new.txt`, and the listing written in
`installer/msix/listing` (below, ADR 0041); nothing else of the listing changes. Then it puts the site online. If the app is not
sent, the site is not deployed either. The Store checks the new version, usually within a day or
two, and then updates everyone. `./mosaik.sh --app` sends the app alone; `.\mosaik.ps1 publish`
does the same from PowerShell.

### The Store's key, in `.env.local`

The key of the Partner Center account's Microsoft Entra application "Mosaik Store publishing"
goes in `.env.local` at the top of the repository (`mosaik/.env.local`), as in a web project:

```
MOSAIK_STORE_KEY=the key
```

Git ignores that file, so it is never committed, pushed or packed into the one-file `mosaik.ps1`:
the repository is public, and whoever has the key can publish to the Store. The application's
tenant and client IDs are in `installer/msix/identity.json`: they are not secrets. Without
`.env.local` (or `MOSAIK_STORE_KEY` set), the first run asks for the key before anything else
and keeps it for your Windows user, encrypted (`%APPDATA%\Mosaik\store-key.xml`).

How it was set up, on 2026-10-13:

1. Partner Center, signed in with the developer account (the Microsoft account that owns
   Mosaïk), **Account settings**, **Tenants**, **Associate Microsoft Entra ID**, signed in there
   as `dev@gimz.onmicrosoft.com`: the tenant `gimz.onmicrosoft.com` is linked to the account.
2. Partner Center, signed in as `dev@gimz.onmicrosoft.com` (Manager): **Account settings**,
   **User management**, **Microsoft Entra applications**, **Create Microsoft Entra
   application**: "Mosaik Store publishing", reply URL `https://gimzdev.com/mosaik-store`
   (Partner Center refuses one on onmicrosoft.com), the **Manager (Windows)** role.
3. The application, **Add new key**: the key, shown once.

Keys expire: Partner Center shows when. Make a new one the same way (step 3) and give it with
`.\installer\msix\publish.ps1 -NewKey`.

### The Store listing, in `installer/msix/listing`

What the Store shows goes with the app, from files beside `publish.ps1`: one folder per language
of the listing (`en-us`). Each file replaces that part of the listing; a part without its file
stays as Partner Center has it, and without the folder only the package and "What's new" change.

- `description.txt`: the description (10,000 characters at most).
- `short-description.txt`: the short description at the top of the listing (1,000 characters at
  most; some views show its first 270).
- `features.txt`: the features, one per line (20 at most, 200 characters each).
- `keywords.txt`: the search terms, one per line (7 at most).
- `screenshots/*.png`: the screenshots, in the order of their names (10 at most), captioned by
  the lines of `screenshots/captions.txt` in the same order. They replace the listing's
  screenshots; its other pictures (logos, promotional art) stay. With no PNG there, the
  screenshots stay as they are.

A folder for a language the listing has not (one the package does not declare) is left out, saying
so, and a listing the Store would refuse sends nothing. `en-us` has the description, features and
search terms of 2026-10-26, the short description of 2026-10-28 and no screenshots, so the Store
keeps the ones it has.

### The submission, for reference

Apps and games, Mosaïk (Store ID `9PHR0L7S1QVD`). 1.0.0 was published on 2026-10-12; each new
submission is a copy of the last one, so only the package and "What's new" change.

The rest of the submission, for reference (already set by submission 1):

- **Pricing and availability**: free, all markets.
- **Properties**: category Productivity; privacy policy `https://xn--mosak-fta.com/conditions`;
  website `https://xn--mosak-fta.com`; support `https://github.com/gimzdev/mosaik/issues`.
- **Age ratings**: the questionnaire (no user-generated content, no purchases, no data shared).
- **Packages**: the `runFullTrust` capability, when asked why:
  "Mosaïk is a desktop app that arranges the windows of the user's other apps (moves, resizes,
  groups them into tabs) and registers one global keyboard shortcut. It needs full trust to see
  and move those windows. It makes no network requests."
- **Store listing** (English): in `installer/msix/listing/en-us` since 2026-10-26 (ADR 0041).
  Submission 1 had:
  - *Description*: "Mosaïk gives every project its own workspace: your apps and windows arranged
    and ready in one click. See your screen as it will be before anything moves: Arrange lays
    it out for what you are doing, or pick one of eight layouts. Carve the screen into named,
    coloured zones; drag a window near a zone and it is pulled in, resize it and the zones
    beside it make room. Stack any windows into tabs, browser tabs included, and flip between
    them. Save a workspace once and bring it back from the tray, the side menu at the screen's
    edge or Win+Alt+Space: positions, sizes and tabs return together, and missing apps start.
    Free and open source. No account, no telemetry, no network requests: your layouts stay on
    your computer."
  - *Features*: Zones that belong to the project, not the monitor · Arrange: every window in its
    place in one click, with undo · Eight layouts, shown on your own windows before anything
    moves · Sets of the apps you use together, opened in one click · Tabs for any app, not only
    the browser · One shortcut to switch workspaces · Works across monitors and scales · Your
    layouts stay on your computer · A safety net brings back every window, always.
  - *Screenshots*: at least one, 1920x1080, of Mosaïk in use (Home with a layout chosen, zones
    with tags, a tab strip, the switcher, the side menu).
  - *Search terms*: window manager, workspaces, tabs, zones, tiling.
- Submit by 2027-01-11 or the reserved name is released (done: 1.0.0 is published).

## 4. After the Store has the new version

- The Download buttons install from the Store's web installer; check one on a clean account.
- The site: `./mosaik.sh --deploy` (already done by the line in section 3).
- GitHub: from the folder above the repositories, `./deploy.sh push mosaik` force-pushes this
  repository to `master` as it is (its README included); the full `./deploy.sh` does every
  repository, and leaves Mosaïk's README as it is written here. Then, if wanted, a release
  tagged with the version (`v1.0.2`) with that version's section of `CHANGELOG.md` (no files to
  attach: Mosaïk installs from the Store).
- `winget install 9PHR0L7S1QVD` works through winget's Store source once the listing is live.
