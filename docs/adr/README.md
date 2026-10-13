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
| [0011](0011-tags-in-title-bars.md) | Tags and tab strips in each window's own title bar, read from what the app answers; since 1.1 a window alone is never lowered: a tag with little room is cut to its zone's initial, else none; a strip always sits at the right end of its title bar (ADR 0017) |
| [0012](0012-every-kind-of-app.md) | Borderless app windows managed, fixed-size windows never resized, full screen left alone |
| [0013](0013-zones-follow-a-resize.md) | Resizing a window in a zone moves the lines between zones; apps that keep a larger size push back |
| [0014](0014-arrange.md) | Arrange: a pure planner puts every window in the zone that suits it, with undo |
| [0015](0015-quick-menu.md) | The quick menu: a native line at the screen's edge that opens on hover and never takes the keyboard |
| [0016](0016-home-page.md) | A home page that shows what Arrange, or a layout, would do before it does it |
| [0017](0017-arrange-minimises-and-moves-only-when-asked.md) | Arrange minimises what it has no zone for; windows move only on the user's actions; strips always in title bars; a quicker quick menu |
| [0018](0018-a-workspaces-apps.md) | A workspace's apps, listed and edited on the Workspaces page |
| [0019](0019-arrange-lays-out-for-the-task.md) | Arrange lays every screen out for its task, the main window the most room; no window gets a zone it cannot fit (since ADR 0021, the zones make room); proper app names |
| [0020](0020-windows-edge-to-edge.md) | Windows meet edge to edge, without coloured rings by default |
| [0021](0021-zones-make-room.md) | Zones make room for a window too big for them, as little as it needs; a window is minimised only when the windows cannot all fit |
| [0022](0022-sets-of-apps.md) | Sets of the apps installed that go together, opened in one click; the windows open sorted into workspaces the first time |
