# 0042. How Mosaïk could earn

- Status: proposed (the owner's choice; nothing is built)
- Date: 2026-11-11

## Context

Mosaïk is free, on the Microsoft Store and on GitHub under CC0, with no account, no telemetry and no
network request (ADR 0008, the site's privacy page). Its developer needs income, and the ship plan
of 2026-11-11 asks what Mosaïk could earn without breaking what it promises, and asks for no code.
Three ways are open.

## Options

**1. Stay free, with supporters.** Patreon and a one-time tip on gimzdev.com/support, a "Rate
Mosaïk" item in About and the tray menu that opens the Store's review page (part 5.3 of the plan),
and the Patreon link that is already there.
- Promises kept: all of them. The app opens a web page only when the person clicks.
- Work: an item in two menus, a line in the changelog. Nothing to maintain.
- Earns: little at first; reviews and ratings also bring Store installs.

**2. A one-time "Supporter" add-on in the Store.** A durable add-on bought in Mosaïk's own Store
page, which unlocks nothing: a thank-you in About, perhaps an extra theme.
- Promises: "free" stays true. "No network request" needs a footnote: buying or checking the add-on
  goes through Windows' Store APIs (`Windows.Services.Store`), which reach Microsoft's servers when
  the person opens the purchase or Mosaïk asks for its licence. Checking only when the About page is
  opened keeps that to a click.
- Work: the add-on in Partner Center, the Store API calls behind a flag, tests against the Store
  stand-in (`store-standin.mjs`), an ADR amending 0008 and the privacy page.
- Earns: one-time payments, minus Microsoft's Store fee on its own commerce (read the App Developer
  Agreement's current rate before deciding).

**3. A Pro tier.** Features held back behind a paid licence (more workspaces, sync, rules).
- Promises: "free" and "open source under CC0" stop meaning what they mean now; a licence check is
  a network request every time it runs, and anyone can build the source without it.
- Work: the most: licence checks, a second build, support for paying users, a pricing page.
- Earns: the most, if the features are worth paying for; but part 5.3's reviews and the CC0 source
  pull the other way.

## Recommendation

Option 1 now; option 2 once the Store shows steady installs and the site gets traffic of its own
(the plan's numbers: mosaïk.com had 23 visitors from October 26 to November 11). Not option 3: it
breaks two promises that are part of why people pick Mosaïk.

## Consequences

- Nothing in the app changes until the owner chooses. Option 1's menu item is part 5.3 of the plan.
- Option 2, if chosen, gets its own ADR, amending 0008 and the privacy page, before any code.
