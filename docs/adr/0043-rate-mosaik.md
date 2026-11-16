# 0043. Rate Mosaïk opens the Store's review page

- Status: accepted
- Date: 2026-11-12

## Context

Ratings and reviews help people find Mosaïk in the Microsoft Store, and its developer asked for a
way to leave one (the ship plan of 2026-11-11, part 5.3; option 1 of ADR 0042). Mosaïk makes no
network request and interrupts nobody (ADR 0008, the site's privacy page).

## Decision

A "Rate Mosaïk" button in About, beside Support on Patreon, and a "Rate Mosaïk" item in the tray
menu, just above it. Either opens `ms-windows-store://review/?ProductId=9PHR0L7S1QVD`, Mosaïk's
rating page in the Store app, through the shell, the way the Patreon link opens the browser
(`open_link` takes `review`, as it takes `patreon`). Nothing asks for a rating: no prompt after a
number of days or uses, no badge, no reminder.

## Consequences

- Mosaïk still sends nothing: Windows opens the Store, which is the Store's own connection.
- A PC without the Store app shows the error "The Microsoft Store did not open", like a PC without
  a browser for the other links.
- Nothing to maintain beyond the Store ID, which is fixed for the app's life.
