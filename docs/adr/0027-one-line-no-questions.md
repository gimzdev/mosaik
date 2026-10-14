# 0027. One line ships both, with nothing to answer once the Store's keys are kept

- Status: accepted
- Date: 2026-10-13
- Amends: ADR 0025 and ADR 0026

## Context

ADR 0026 made the send by hand the default, with the Store's API only after `-SetKeys` and a
"Send it now?" each time. The owner: "new version is the simplest shipping possible, i got app on
machine i send app to store alright? dont add paperwork for me i do ./mosaik1.sh --deploy --app
and the site & app are both online updated". They are making a Microsoft Entra tenant for the
Store's keys.

## Decision

- **No question when the keys are there.** `publish.ps1` sends through the Store's API without
  asking: `--app` on the command line is the yes.
- **The keys asked once, on the first run.** Without keys, it asks for the tenant ID, client ID
  and key in the terminal, says where they are in Partner Center, keeps them for that Windows
  user (DPAPI) and sends. `-SetKeys` asks again, for a new key.
- **By hand only when asked for.** Enter at the first question sends it by hand that time (ADR
  0026's steps). `-Yes` (a script) never asks: without keys, nothing is sent.

## Consequences

- After the first run, `./mosaik1.sh --deploy --app` is the whole release: tests, build,
  package, the Store's submission, the site. Microsoft's review of each version (usually within a
  day) is the only wait, and it asks nothing of the owner.
- `installer/msix/test-publish.ps1` checks it against its stand-in: keys in the environment, keys
  given on the first run and kept, the next run with nothing asked, the question skipped (by
  hand) and `-Yes` without keys.
