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
  moved, not saved into a workspace, whatever the user's rules say, until it leaves full screen;
  then it is an ordinary window again. Full screen is a frame within 2 px of its monitor's
  bounds, not maximised (`WindowFacts::full_screen`); the desktop and the shell's windows are
  never taken for one.
- **A window without a frame to resize it by keeps its own size.** Mosaïk only moves it: to
  the middle of its zone, or of its group's place (`fit_size`, ADR 0005), also when it comes
  back from minimised (`mosaik_win::place::move_frame`, with a Windows test). It is in a zone
  when its centre is in the zone and at least 70% of it is inside the zone's area
  (`holds_fixed`), since it cannot fill the zone. Its ring goes around the window itself.
- **Its tag** goes in its title bar when there is room, as for any window (ADR 0011). When it
  needs a band, the band sits right above the window, not at the top of the zone
  (`zone::band_above`).

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
- Steam itself was not dragged in the tests: its window was open on the owner's screen, and the
  probes touch only windows they open. Its classification was read: managed.
