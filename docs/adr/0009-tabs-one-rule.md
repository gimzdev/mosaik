# 0009. One rule for what tabs show, applied on a hiding thread

- Status: accepted
- Date: 2026-10-11

## Context

The first tab implementation hid and showed windows wherever an action needed it: clicking a
chip, dropping a window, pulling a tab out, a window closing, a switch. An adversarial review of
it found windows that could stay hidden for good: the old group of a tab dragged into another
group, hidden tabs of a workspace being left, background tabs of a group whose strip could not
be drawn (tags off, no room, maximised, drawing failed), a tab whose show failed but was
forgotten anyway. It also found calls into other processes on the engine thread: the shell cloak
is a COM call into Explorer, minimising waits for the window's app.

## Decision

- What shows is decided in one place, `mosaik_core::tabs::plan_tabs`, from plain data: each
  group's open tabs, the tab that should show, and how its other tabs can be reached (strip on
  screen; later, with its minimised or maximised visible tab; or not at all). A window stays
  hidden only while it is a background tab of a group whose other tabs can be reached. The engine
  applies the plan in `sync_tabs` after anything that changes groups or what can be drawn; every
  other window it hid comes back.
- Hiding and showing run on a thread of their own (single-threaded COM apartment, as measured in
  ADR 0003). The engine sends a batch and waits at most 1.5 s, handling its messages meanwhile;
  answers that come late are applied on the next pass. Requests run in order, so a late answer
  never undoes a newer request.
- The journal is written once per batch, before anything hides; the window marker is set before
  the window hides. A show that fails keeps the window in the journal (and is told); a hide that
  fails makes its group show all its tabs.
- The watchdog is checked before every batch of hides and started again (three times at most)
  if it stopped; without it nothing hides.

## Consequences

- One function to test for "never lose a window": its rule has unit tests, and the layout
  operations that feed it have a property test.
- Each pass re-reads the groups and draws the strips again (cached when nothing changed): a few
  hundred microseconds per window event that touches a tab.
- A tab whose app is not responding keeps its group stacked until it answers.
