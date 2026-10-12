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
| [0009](0009-tabs-one-rule.md) | One rule for what tabs show, applied on a hiding thread |
| [0010](0010-drags-and-other-arrangers.md) | Move or resize from the pointer Windows shows; Windows' snapping and other arrangers named once, never fought |
| [0011](0011-tags-in-title-bars.md) | Tags and tab strips in each window's own title bar, read from what the app answers; a band only where there is no room |
| [0012](0012-every-kind-of-app.md) | Borderless app windows managed, fixed-size windows never resized, full screen left alone |
