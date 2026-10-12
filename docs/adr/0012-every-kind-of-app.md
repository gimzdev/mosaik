# 0012. Every kind of app window: borderless apps, fixed-size games, full screen

- Status: accepted
- Date: 2026-10-12

## Context

The owner: Mosaïk must work with practically every app, "like Claude, Steam", "even games like
Vampire Survivors, idle clickers". Three kinds of windows were left out or mishandled:

- **Borderless app windows.** Steam's library, game launchers and many games draw their own
  title bar in a window without Windows' frame (SDL, CEF, Unity, Godot): a popup window
  (`WS_POPUP`). Mosaïk took every popup for a splash screen or a menu and left it alone. Steam's
  library window, read on the owner's PC (job 0078): class `SDL_app`, program
  `steamwebhelper.exe`, a popup with a caption, a frame to resize it by and a minimise button,
  in Alt+Tab.
- **Windows of a fixed size.** Most games in a window, and many small tools, have no frame to
  resize them by (`WS_THICKFRAME`). Windows still lets another program resize them, and many
  take the new size: a game then draws stretched, blurred or cut off. Job 0077: the probe's
  fixed-size game, dragged into a zone, was stretched to fill it.
- **Full screen.** A game or a video in full screen covers its whole monitor, taskbar included,
  without being maximised. Moving it, or framing it, would take it out of full screen or draw
  over it.

## Decision

- **A popup is an app window when it has something of a window about it**: a caption, a frame
  to resize it by, or a minimise button. A popup with none of them (a splash screen, a menu, a
  toast) stays out. Every other rule is unchanged: it must be in Alt+Tab, have no owner, and so
  on (`mosaik_core::classify`, with tests).
- **A window in full screen is left alone** (`Pending::FullScreen`): no frame, no tag, never
  moved (the mover refuses it), not saved into a workspace, whatever the user's rules say, until
  it leaves full screen; then it is an ordinary window again. Full screen
  (`window::is_full_screen`) is a frame within 2 px of its monitor's bounds, not maximised, and
  without Windows' title bar and resize frame: a window that has both, placed over a whole
  monitor without a taskbar (by Mosaïk, in a zone that large), is not; browsers, players and
  games in full screen have neither. It is checked after the rules that tell an app's window,
  so an overlay over the whole screen is not taken for one, and the desktop and the shell's
  windows never are. A workspace's window in full screen is still the workspace's: opening the
  workspace leaves it as it is (not moved, raised or minimised) and does not start its app again,
  and leaving the workspace does not minimise it.
- **A window without a frame to resize it by keeps its own size.** Mosaïk only moves it: to
  the middle of its zone, or of its group's place (`fit_size`, ADR 0005, which never changes a
  size: a window larger than the screen starts at its top-left), also when it comes back from
  minimised (`mosaik_win::place::move_frame`, with a Windows test). It is in a zone when its
  middle is in the zone, or the zone's middle is in it, and it overlaps the zone's area by 70% of
  as much as its size allows (`holds_fixed`): all of it in a larger zone, all of a smaller zone.
  A window that also covers a neighbouring zone stays in the zone it was put in. Its ring goes
  around the window itself.
- **Its tag** goes in its title bar when there is room, as for any window (ADR 0011). When it
  needs a band, the band sits right above the window, not at the top of the zone
  (`zone::band_above`).
- **A band works as its window's title bar.** Solid, it looks like part of the window, so a
  press on it is a press on the window's title bar: the window comes to the front, and dragged,
  it follows the pointer, with the zones showing and tags and strips taking it as for any drag.
  Mosaïk moves it, from a thread of its own (`mover::Follower`), so an app slow to move never
  holds Mosaïk up; it never takes the window off every screen (it is held above itself, so the
  pointer alone could); Esc puts it back, and Mosaïk holds Esc while the button is down so it
  does not reach the app, as Windows' own move does. The same goes for a strip's band beside
  its chips, which moves the whole group, and for the zone's tag at a strip's start. Windows'
  own move loop cannot be started from another app's window: it reads the button from its own
  thread's input, which never saw the press (job 0080: posted `SC_MOVE`, nothing moved).
- **When zones overlap**, a window belongs to the one whose middle is nearest its own, then the
  smallest (`workspace::holding`).

## Measurements

The probe's own window stands in for each kind (`mosaik-probe workspace --apps --only kinds`),
dragged with the real mouse into a zone, onto another window's tag, its tab clicked, then
pulled out (jobs 0077 and 0078, 1920x1080 at 100%):

```
borderless app, its own 32 px title bar (Steam-like)   0 px at every step; tag in its own title bar
game in a fixed window (classic title bar)             kept 626 x 413 where 856 x 508 was given,
                                                       in the middle, 0 px; tag in its title bar
borderless game of a fixed size                        kept 640 x 420, in the middle, 0 px; tag in
                                                       its own title bar
borderless game of a fixed size, moving itself,        kept 560 x 360, in the middle, 0 px; a solid
no title bar to grab                                   band right above it with its tag
game in full screen                                    left alone ("pending: full screen"); the
                                                       workspace saved 3 windows, not 4; untouched
```

Before the change the same fixed-size windows filled their zones (job 0077), and the new
Windows test failed on the old code (job 0078: 626 x 473 became 1060 x 1032).

A review of the change found, and tests now cover: a fixed-size window larger than the work
area was shrunk to it (`fit_size` fitted sizes as well as places); one larger than its zone was
not counted in it, and one over two zones could be counted in the wrong one; opening a workspace
whose window was in full screen started its app again; a window in full screen still wore its
frame and could be moved into a band; a window placed over a monitor without a taskbar, with a
small gap, was taken for one in full screen; the solid band let presses through to whatever was
behind it.

## Consequences

- Steam, launchers and borderless games are managed like other windows, and their title bars
  are read the same way: an app that draws its own title bar answers "caption" where it can be
  dragged, or Windows could not move it (VS Code, measured in ADR 0011; SDL, which Steam uses,
  answers so for its draggable areas).
- A fixed-size window smaller than its zone leaves the rest of the zone empty. Dropping another
  window there makes them tabs, as anywhere.
- An app that resizes its window with its own code, without a resize frame (rare), is treated
  as fixed: it keeps the size the user gives it.
- A game that changes the display's mode for its full screen makes Mosaïk fit the open
  workspace to that mode, and again when the game ends: the windows end in their places.
- An app that keeps Windows' title bar and resize frame in full screen cannot be told from a
  window placed over a whole monitor: it is never moved below a band (no window over its whole
  monitor is), but it wears its frame, and opening its workspace moves it like any window.
- Steam itself was not dragged in the tests: its window was open on the owner's screen, and the
  probes touch only windows they open. Its classification was read: managed.
