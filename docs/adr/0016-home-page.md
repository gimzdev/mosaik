# 0016. A home page that shows what Arrange would do before it does it

- Status: accepted
- Date: 2026-10-12

## Context

The owner asked for an app "usable by an absolute noob", with animation and layouts on its home
page, "really straightforward", minimal and efficient. Until now the window opened on the
Workspaces page, which assumes the user knows what a workspace is; Arrange was one button among
others there, and layouts lived only in the zone editor. A beginner had no way to see what a
click would do to their windows before it did it.

## Decision

- **The window opens on Home** (after the welcome, the first time). One task on it: tidy the
  screen. Its parts, top to bottom: a picture of the main monitor, the button "Arrange
  windows" (its name in the tray and the quick menu too), ten layouts as tiles, the saved
  workspaces (up to six, each with Open), and one line on the switcher's shortcut and the
  quick menu. The Workspaces page keeps managing them (rename, shortcut, zones, delete); its
  empty state leads back home.
- **The picture is drawn from `preview_arrange`**, a new command that runs the same preparation
  as `arrange` (`prepare_arrange`) and changes nothing, so what the picture shows and what the
  click does cannot disagree. It sends the main monitor's work area, the zones as they would be,
  and each window on it: where it is now, where it would go (`then`, `null` to stay) and whether
  it would become a tab; windows on other monitors are counted, not drawn. A window of a fixed
  size is shown at its own size, centred at the top of its place, as `move_frame` puts it.
- **Pointing shows, clicking does.** While the pointer or the keyboard focus is on Arrange or a
  layout, the windows glide from where they are to where they would go (460 ms, one after the
  other by 45 ms), the zones appear, windows that would become tabs fade, and each window wears
  its zone's colour and name, as on screen. Leaving puts the picture back. With reduced motion,
  the picture changes at once. A click arranges; Undo, next to the button, puts everything back.
  Right after the welcome the picture shows once what Arrange would do, without being asked.
- **A layout replaces the main monitor's zones.** `arrange` and `preview_arrange` take an
  optional layout id (`Preset`, the zone editor's presets, checked against the interface's list
  by a test). The planner (`plan_with`, `Chosen`) gives the main monitor that layout's zones,
  every window there goes in the one that suits it (ADR 0014's scores), windows more than its
  zones become tabs, and other monitors arrange as before. The open workspace's zones on that
  monitor go (`editing::without_zones`, windows in them keep their places until they move), so
  Undo brings them back with the windows. With no workspace open, a new one is made.
- **Protocol 4**: `arrange { preset? }`, `preview_arrange { preset? }` and the `preview` reply.
  `mosaikctl arrange [<layout>]` and `mosaikctl preview [<layout>] [--json]` do the same from a
  terminal.
- **One name for the window**: the tray's and the quick menu's "Settings" row is now "Open
  Mosaïk": it opens the window, on its home page when it was closed.
- **The window is where a beginner looks for it.** Opened by hand (the Start menu, the
  installer's last step), Mosaïk shows its window; started with Windows, it waits in the tray
  (the Run key's entry carries `--background`; the package's startup task is told by its
  activation, `AppInstance::GetActivatedEventArgs`). A click on the tray icon opens the window;
  it opened the switcher, which stays in the icon's menu and on its shortcut.
- **The welcome shows rather than tells**: a made-up screen arranges itself once (again on
  "Show it again"), then three steps (Arrange, Keep, Come back) and one choice, starting with
  Windows.

## Consequences

- Each pointing asks the engine for a preview: it reads the windows on screen, as Arrange does.
  An answer that comes after the pointer moved on is dropped. Measured with `mosaik-probe smart`
  (`docs/STATUS.md`).
- The picture is a model, not a screenshot: no window's content is captured or shown.
- A window that will wear its group's strip above it (a tab group whose visible tab has no room
  in its title bar) can land a band lower than the picture shows: whether the strip fits is
  known only once the group exists.
- Only the main monitor is pictured and takes a layout; the others are arranged by Arrange's own
  choice, as the caption says.

## Amended by ADR 0017 (2026-10-12)

Arrange and the layouts make no tabs: a window they have no zone for would be minimised, and the
picture shows it shrinking to the taskbar (`PreviewWindow.minimized`, protocol 5). The page says
less: one line under its title, no explanation under Arrange, and the switcher's shortcut alone
at the bottom.
