# 0008. Distribution through the Microsoft Store, downloaded from mosaïk.com

- Status: accepted (the owner chose it on 2026-10-11)
- Date: 2026-10-11

## Context

The brief planned a per-user NSIS installer on GitHub Releases, signed with Azure Artifact
Signing, plus winget and an opt-in updater of Mosaïk's own. Then:

- **Smart App Control blocks unsigned programs outright**, with no "run anyway". It is on on the
  owner's PC and blocked a freshly built `mosaikctl.exe` (Code Integrity event 3077, policy
  `{0283ac0f-fff1-49ae-ada1-8a933130cad6}`), while letting other new builds through: unsigned
  files are let in or not depending on the cloud's verdict. SmartScreen warns about unsigned
  downloads everywhere else. Every program Mosaïk ships has to be signed.
- The owner's first rule: one click from mosaïk.com, minimal, nothing extra to depend on, and no
  running cost. Creating the Azure account was blocked by Microsoft's anti-fraud check.

Options:

| Option | Cost | Notes |
| --- | --- | --- |
| Azure Artifact Signing | $9.99/month | Identity validation; individuals in Canada eligible. Plus our own updater, update key and update check. |
| SignPath Foundation | free | Requires an OSI-approved licence (CC0 is not one); the publisher shown is "SignPath Foundation"; needs an established reputation. |
| A code-signing certificate from a CA | yearly fee | Key on a token or a cloud service; SmartScreen reputation still builds slowly. |
| Unsigned download | free | Blocked by Smart App Control, warned by SmartScreen. |
| **Microsoft Store** | **free** | Free for individuals since September 2025 (ID and selfie once). Microsoft signs the MSIX, hosts it and updates it. The Store's *direct* web installer lets a site's own Download button fetch a small Microsoft-signed installer in the browser; it installs the app without opening the Store and starts it. |

## Decision

- Mosaïk ships as an MSIX package on the Microsoft Store, published by "Gimzdev". The name
  "Mosaïk" was reserved on 2026-10-11 (Store ID `9PHR0L7S1QVD`, package name `Gimzdev.Mosak`;
  all identity values in `installer/msix/identity.json`). A reserved name is kept only if the
  app is submitted within three months: by 2027-01-11.
- The Download button on mosaïk.com links to the Store's direct web installer (our button, no
  Store page, no script from Microsoft on the site).
- Updates come from the Store. Mosaïk has no updater of its own, no update key and no update
  check: it makes no network request at all.
- `winget install <Store ID>` works through winget's Store source; no manifest of our own.

## Consequences

- Every release goes through Store certification (usually a day or two). Publishing stays the
  owner's action.
- The app is packaged with `makeappx` in CI. Inside the package, `%APPDATA%\Mosaik` is
  redirected by Windows to the package's own folder and removed on uninstall; Mosaïk does not
  need to know. "Start with Windows" uses the package's startup task (the user can turn it off
  in Task Manager too), and `mosaikctl` is an app execution alias.
- The watchdog, the named pipe, global hotkeys, window hooks and moving other apps' windows all
  work in a full-trust desktop package.
- The installer needs an internet connection, and organisations that block `get.microsoft.com`
  cannot use it.
- Development builds stay unsigned, so Smart App Control can block a test build on the owner's
  PC; test jobs fall back to the debug build when that happens.
- `/conditions` on the site no longer lists an update check; the brief's NSIS, Azure and updater
  items in section 10 are replaced by this record.
