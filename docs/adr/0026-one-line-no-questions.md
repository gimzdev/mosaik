# 0026. One line ships both, with nothing to answer once the Store's keys are kept

- Status: accepted
- Date: 2026-10-13
- Amends: ADR 0025

## Context

ADR 0025 sends the package through the Store submission API after asking "Send it now?" each
time, and asks for the Store's keys only on a first run in a terminal. The owner: "new version is
the simplest shipping possible, i got app on machine i send app to store alright? dont add
paperwork for me i do ./mosaik1.sh --deploy --app and the site & app are both online updated".
The keys belong to a Microsoft Entra application of the Partner Center account, which needs a
Microsoft Entra tenant: the owner made one on 2026-10-13 (`gimz.onmicrosoft.com`).

## Decision

- **No question when the keys are there.** `publish.ps1` sends through the Store's API without
  asking: `--app` on the command line is the yes.
- **The keys asked once, on the first run.** Without keys, it asks for the tenant ID, client ID
  and key in the terminal, says where they are in Partner Center, keeps them for that Windows
  user (DPAPI) and sends. `-SetKeys` asks again, for a new key.
- **Without keys, nothing is sent.** Nothing typed (or `-Yes`, a script, which never asks) ends
  as a failure that says what is needed, so the site is not deployed either.

## Consequences

- After the first run, `./mosaik1.sh --deploy --app` is the whole release: tests, build,
  package, the Store's submission, the site. Microsoft's review of each version (usually within a
  day) is the only wait, and it asks nothing of the owner.
- `installer/msix/test-publish.ps1` checks it against its stand-in: keys in the environment, keys
  given on the first run and kept, the next run with nothing asked, no keys given, and `-Yes`
  without keys.
