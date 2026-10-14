# 0026. The new version sent by hand in Partner Center, unless the Store's API has keys

- Status: accepted; amended by ADR 0027 (the keys asked on the first run, no question after)
- Date: 2026-10-13
- Amends: ADR 0025

## Context

ADR 0025 sends the package through the Store submission API, which acts for the Partner Center
account through a Microsoft Entra application; the account needs a Microsoft Entra tenant for
it, and the owner's developer account (made with a personal Microsoft account) had none. On
2026-10-13 the owner made one from Partner Center (Tenants, Create Microsoft Entra ID): Microsoft's
sign-up asks for a payment card, refused the owner's (a French card, an account in Canada), set
the tenant's country to the United States, and blocked the tenant ("Access to tenant is denied",
AADSTS5000228, at Partner Center's sign-in); only Microsoft can lift that. Another sign-up was
sent back to the blocked account, whose contact address is the owner's. The owner: "lets do the
best way".

## Decision

- **By hand by default.** Without keys, `installer/msix/publish.ps1` (run by `mosaik.ps1 publish`
  and `mosaik1.sh --app`, after the tests, the build and the package) opens Partner Center on
  Mosaïk's page in the browser (where the owner is signed in), shows the package in File Explorer
  and puts "What's new" on the clipboard, says the four steps (Start update; the package in, the
  older one out; "What's new" pasted; submit), and asks "Submitted?". A no, or no answer, ends
  as a failure, so `mosaik1.sh --deploy --app` puts the site online only with the app sent.
- **The API when keys are set.** `publish.ps1 -SetKeys` asks for the Entra application's tenant
  ID, client ID and key and keeps them (encrypted, for that Windows user); from then on the send
  is ADR 0025's, after a yes, or with `-Yes` from a script. The keys are no longer asked for
  unprompted. `-Yes` without keys fails: nobody would be there to send it by hand.

## Consequences

- One line still ships both (`./mosaik1.sh --deploy --app`); the upload is about two minutes of
  the owner's clicks per release, the rest is the script's.
- Nothing about the app, its listing or its users depends on the way it is sent.
- `installer/msix/test-publish.ps1` checks both ways: the API against its stand-in, and by hand
  with what it would open written instead (`MOSAIK_STORE_OPEN=print`) for the three answers.
