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
| [0011](0011-tags-in-title-bars.md) | Tags in each window's own title bar, read from what the app answers; a window alone is never lowered: a tag with little room is drawn compact, else none (ADR 0029); tab strips are in a band since ADR 0027 |
| [0012](0012-every-kind-of-app.md) | Borderless app windows managed, fixed-size windows never resized, full screen left alone |
| [0013](0013-zones-follow-a-resize.md) | Resizing a window in a zone moves the lines between zones; apps that keep a larger size push back |
| [0014](0014-arrange.md) | Arrange: a pure planner puts every window in the zone that suits it, with undo |
| [0015](0015-quick-menu.md) | The quick menu: a native line at the screen's edge that opens on hover and never takes the keyboard |
| [0016](0016-home-page.md) | A home page that shows what Arrange, or a layout, would do before it does it |
| [0017](0017-arrange-minimises-and-moves-only-when-asked.md) | Arrange minimises what it has no zone for; windows move only on the user's actions; a quicker quick menu (strips in title bars until ADR 0027) |
| [0018](0018-a-workspaces-apps.md) | A workspace's apps, listed and edited on the Workspaces page |
| [0019](0019-arrange-lays-out-for-the-task.md) | Arrange lays every screen out for its task, the main window the most room; no window gets a zone it cannot fit (since ADR 0021, the zones make room); proper app names |
| [0020](0020-windows-edge-to-edge.md) | Windows meet edge to edge, without coloured rings by default |
| [0021](0021-zones-make-room.md) | Zones make room for a window too big for them, as little as it needs; a window is minimised only when the windows cannot all fit |
| [0022](0022-sets-of-apps.md) | Sets of the apps installed that go together, opened in one click; the windows open sorted into workspaces the first time |
| [0023](0023-a-simpler-app.md) | A simpler app: Home and Settings only; three windows to a view, a set's other apps as tabs; every app started once and placed as soon as it shows; the quick menu where the pointer stops, and never in a game |
| [0024](0024-the-window-rebuilt.md) | The window rebuilt (the screen as the layout chosen would leave it, Smart and ten layouts, windows dragged onto each other to trade places, then Arrange; workspaces beside it); dark by default; apps by their own icons or none; toasts and notes instead of alerts; the side menu; the site's first home page back |
| [0025](0025-sending-to-the-store.md) | A new version sent to the Store from a script (`mosaik.ps1 publish`, `mosaik.sh --app`) with the Store submission API; the Entra application's key kept encrypted for the user |
| [0026](0026-one-line-no-questions.md) | One line ships both with nothing to answer: the Entra application's IDs in the repository, its key asked once on the first run and kept, then no question |
| [0027](0027-tab-strips-in-a-band.md) | A tab group's strip is in a band above it, never in a title bar: a chip for every tab, to drag out |
| [0028](0028-zones-numbered-and-workspaces-previewed.md) | Zones numbered 1, 2, 3 by place; a workspace's page and Home show how it opens (Open, not a click); opening one minimises the other windows on its monitors |
| [0029](0029-for-people-new-to-mosaik.md) | For people new to Mosaïk: bigger text and controls, a preview that says so, Back, named layouts, one place for results; an older Mosaïk asked to quit through its engine's window; scans of the windows shown only, waits that end when the answer comes |
| [0030](0030-arrange-arranges-save-saves.md) | Arrange arranges, Save saves: Arrange makes and changes no workspace; its zones are kept apart until a workspace is opened or saved, and Save saves the zones on screen |
| [0031](0031-the-picture-is-where-zones-change.md) | The picture shows each zone once (empty ones too, numbered and coloured) with its windows' title bars; windows dragged and lines between zones dragged there, on Home before Arrange windows and on a workspace's picture saved at once; the layouts all in view, without the rows; Arrange windows (Move windows since ADR 0032); no welcome; a workspace's apps listed by zone |
| [0032](0032-windows-stack-and-trade-places-in-a-zone.md) | Windows a layout has no zone for stack in its zones rather than being minimised; two windows of one zone trade places in it (front to back, or in a tab strip), on Home and on a workspace's picture; Move windows; Home's title, picture and layouts one width; an arranged screen stays as it is, and zones take the colours their windows wear |
| [0033](0033-drops-join-lines-move-no-window-layouts-for-workspaces.md) | A window dropped onto a zone joins it behind its windows; a line dragged moves no window to another zone; a layout chosen for a workspace shown on Home lays it out, every app kept, with Undo; Add an app, open or not, in one list; a workspace's apps dragged in their list; Like Windows follows Windows; the first run says nothing |
| [0034](0034-the-list-drops-as-the-picture-one-width-page-rename-in-the-menu.md) | A workspace's list takes drops as its picture does (onto an app's name it trades, elsewhere in a zone it joins, behind its apps); its page is one width, the picture's, in the middle; its menu on Home renames it in its row, and Edit zones is on its page |
| [0035](0035-the-one-file-script-builds-the-source-inside-it.md) | The one-file `mosaik.ps1` builds the source inside it: a folder another one unpacked records its source and is brought up to date (the build kept, a changed file kept aside, files dated now); a failed build starts again the Mosaïk that quit for it |
| [0036](0036-two-themes.md) | Two themes, Dark (the default) and Light: "Like Windows" goes, settings that chose it taking the theme Windows has when Mosaïk starts |
| [0037](0037-apps-started-as-the-taskbar-starts-them.md) | A workspace starts an app as Windows' taskbar relaunches it (Git Bash by git-bash.exe, a Chrome profile with its profile), finds a window by the program that relaunches it, and never starts mintty alone, which asks which shell |
| [0038](0038-sample-workspaces-named-for-the-work.md) | Sample workspaces are named for the work done in them (Client work, Side project, Brand design; the site's "Client website" is Client work); pictures of the site are captioned mosaïk.com |
| [0039](0039-scripts-named-after-their-app.md) | Each script is named after its app or site: `mosaik.sh` (the site, was `mosaik1.sh`), and in the owner's `gimz` folder `metaforge.sh`, `dropdate.sh`, `grimoire.sh` and `gimzdev.sh`; no number or version in their names, their copies' or their markers' |
| [0040](0040-one-folder-for-the-app.md) | The one-file `mosaik.ps1` works in one folder, `mosaik` beside it, whatever its version; when that is Mosaïk's Git repository it works in it as it is, never writing into it: on the owner's PC, one folder for the app and one for the site |
