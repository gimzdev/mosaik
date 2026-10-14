# 0025. Sending a new version to the Store from a script

- Status: accepted; amended by ADR 0026 (nothing to answer once the keys are kept)
- Date: 2026-10-13
- Amends: ADR 0008 (the Microsoft Store), docs/RELEASE.md

## Context

The owner, about shipping: "is there a way to add an option so I can run bash with --deploy and
maybe a --app after that which does the app push with it", that is, one line that puts the site
online and sends the new version of the app to the Store: `./mosaik1.sh --deploy --app`.

Until now the app reached the Store by hand: `mosaik.ps1 package`, then a submission in Partner
Center (upload the package, paste "What's new", submit). Microsoft's Store Developer CLI
(`msstore`) publishes the kinds of project it recognises (UWP, WinUI, Electron, Flutter, React
Native, PWAs...); its `publish` command looks for one of them before anything else, and a Tauri
app built with Cargo is not one. The Store submission API it uses underneath takes any package.

## Decision

- **`installer/msix/publish.ps1`** sends the package with the Microsoft Store submission API
  (`manage.devcenter.microsoft.com`), the steps Microsoft documents: an access token for the
  Partner Center account's Microsoft Entra application; the submission left waiting, if any,
  deleted; a new submission, which is a copy of the last one published; the old package marked
  for removal and the new one added, and "What's new in this version" set from
  `installer/msix/whats-new.txt` in every language of the listing (nothing else of the listing
  changes); the package uploaded in a ZIP to the address the submission gives; the submission
  committed, and its status read until the Store has taken it.
- **The Entra application** is made once in Partner Center (Account settings, User management,
  Microsoft Entra applications, with the Manager role; the account needs a tenant for it). Its
  tenant ID, client ID and key are asked the first time and kept for that Windows user only,
  encrypted with DPAPI (`%APPDATA%\Mosaik\store-publishing.xml`), never in the repository;
  `MOSAIK_STORE_TENANT`, `MOSAIK_STORE_CLIENT` and `MOSAIK_STORE_KEY` serve a build server.
- **`mosaik.ps1 publish`** runs the tests, the build and the package, then `publish.ps1`, which
  shows the version and its "What's new" and asks before sending (or `-Yes`); a no, or no answer
  from a script, sends nothing and ends as a failure, so nothing after it runs. The build first
  quits a Mosaïk running from the same folder (Windows keeps its files from being built over) and
  starts it again after. Smart App Control refuses a new build's files for a while (job 0133:
  `mosaikctl.exe` refused while `mosaik.exe` ran): when it refuses `mosaikctl.exe`, Mosaïk is
  stopped instead, its watchdog first (job 0134: stopped alone, Mosaïk left its watchdog showing
  "Mosaïk has stopped" and the build waiting), and the watchdog's quiet rescue brings back its
  windows from the journal (ADR 0006); when it refuses the new `mosaik.exe`, the build says so and
  the publish goes on.
- **`mosaik1.sh --app`** runs that `publish` with the app beside the site script: the
  repository (`mosaik/`) when it is there, else the one-file `mosaik.ps1`. It goes first: when the
  app is not sent, the site is not deployed either. Alone, `--app` sends only the app. In Git
  Bash's own window (mintty), PowerShell runs through `winpty` so it can ask.

## Consequences

- One line ships both: `./mosaik1.sh --deploy --app`. The Store still checks every version
  (usually within a day or two) before anyone gets it; the site is online at once.
- A submission made this way is changed only this way: Microsoft warns that editing it in
  Partner Center meanwhile can leave it unable to be committed (then it is deleted and made
  again, which the next run does by itself).
- The protocol with Microsoft is checked on the owner's PC against a stand-in of the two services
  that records every request (`installer/msix/test-publish.ps1`, part of `mosaik.ps1 test`: jobs
  0132 and 0133), and so are a no and the build around a running Mosaïk
  (`installer/test-mosaik-ps1.ps1`, which found the build stopping at once in job 0132); the real
  submission is the owner's, the first time they send a version.
