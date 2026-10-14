# 0026. One line ships both, with nothing to answer once the Store's key is kept

- Status: accepted
- Date: 2026-10-13
- Amends: ADR 0025

## Context

ADR 0025 sends the package through the Store submission API after asking "Send it now?" each
time, and asks for the Store's keys only on a first run in a terminal. The owner: "new version is
the simplest shipping possible, i got app on machine i send app to store alright? dont add
paperwork for me i do ./mosaik1.sh --deploy --app and the site & app are both online updated".
The keys belong to a Microsoft Entra application of the Partner Center account, which needs a
Microsoft Entra tenant: the owner made one on 2026-10-13 (`gimz.onmicrosoft.com`), linked to the
account the same day, with the application "Mosaik Store publishing" (Manager). Then the owner:
"why should the script asks for keys cant you put them in it".

## Decision

- **No question when the keys are there.** `publish.ps1` sends through the Store's API without
  asking: `--app` on the command line is the yes.
- **The IDs in the repository, the key in `.env.local`.** The application's tenant and client
  IDs are in `installer/msix/identity.json`: they are not secrets. The key is: the repository is
  public (and the one-file `mosaik.ps1` carries it), and the key alone publishes to the Store, so
  it never goes in a file of it. It goes in `.env.local` at the top of the repository, as in a
  web project (the owner, after the prompt failed him: "lets just be normal please"): git ignores
  that file, and `pack` leaves it out. Without a kept key, `publish.ps1` asks for the key alone, says
  where Partner Center makes one, keeps it for that Windows user (DPAPI) and sends. `-NewKey`
  asks again, for a new one. `mosaik.ps1 publish` asks first (`publish.ps1 -KeyOnly`), so the
  tests and the build that follow need nobody; and since Git Bash's window does not pass a paste
  on to the hidden prompt (the owner's first run, 2026-10-13: "i did both shift insert ... then
  did click paste"), the key copied to the clipboard is taken when nothing comes through.
- **Without the key, nothing is sent.** Nothing typed (or `-Yes`, a script, which never asks)
  ends as a failure that says what is needed, so the site is not deployed either.

## Consequences

- After the first run, `./mosaik1.sh --deploy --app` is the whole release: tests, build,
  package, the Store's submission, the site. Microsoft's review of each version (usually within a
  day or two) is the only wait, and it asks nothing of the owner.
- `installer/msix/test-publish.ps1` checks it against its stand-in: keys in the environment, keys
  given on the first run and kept, the next run with nothing asked, no keys given, and `-Yes`
  without keys.
