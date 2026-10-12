# Architecture decision records

Short records of decisions that shaped Mosaïk, newest last. Each says what was decided, why, and
what it costs. Measurements behind a decision are summarised in the record and in
[`../STATUS.md`](../STATUS.md).

| ADR | Decision |
| --- | --- |
| [0001](0001-one-repository.md) | One repository for the app, the site and the docs |
| [0002](0002-stack.md) | A Rust engine, a native switcher and overlays, web windows only on demand |
| [0003](0003-hiding-background-tabs.md) | Hiding background tabs with the shell's cloak, minimise as fallback |
| [0004](0004-switcher-hotkey.md) | One global hotkey, Win+Alt+Space, with a fallback list |
| [0005](0005-placing-windows.md) | Placing windows by their visible frame, fitting apps that keep their own size |
| [0006](0006-watchdog-and-journal.md) | A separate watchdog, a journal and a marker on every hidden window |
| [0007](0007-ipc.md) | A per-user named pipe, checked from both ends |
| [0008](0008-store-distribution.md) | Distribution through the Microsoft Store, downloaded from mosaïk.com |
