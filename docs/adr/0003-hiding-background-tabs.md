# 0003. Hiding background tabs with the shell's cloak

- Status: accepted
- Date: 2026-10-11

## Context

A tab group is several top-level windows sharing one rectangle, one visible at a time. Mosaïk
never reparents another app's window (`SetParent` breaks focus, input and DPI handling), so the
background tabs must be hidden some other way, and brought back instantly, drawn, in place,
still maximised if they were.

Candidates: the shell's view cloak (what virtual desktops use, through an undocumented interface),
`DwmSetWindowAttribute(DWMWA_CLOAK)`, minimising without activation, and parking the window off
screen.

## Measurements

`mosaik-probe matrix` on the owner's PC (Windows 11 25H2, 1920x1080 at 100%, 75 Hz, window
animations on), on Notepad, Edge, Chrome, VS Code, Windows Terminal, Explorer, Word and Calculator
(UWP), with a magenta backdrop behind the app so captures show either the app or the backdrop.
Nine hide-and-show cycles per app (runs of 2026-10-11, `docs/STATUS.md`):

| Method | Hidden completely | Hide | Show | First frame drawn after show | Notes |
| --- | --- | --- | --- | --- | --- |
| Shell cloak | 8/8 apps | 1-8 ms | 1-3 ms | 1-3 ms | Calculator twice faded in over ~210 ms when uncloaked after an earlier minimise |
| Minimise (no activation) | 8/8 | 1-16 ms | 1-32 ms | 203-217 ms | the restore animation plays; UWP apps may suspend |
| Park off screen | 8/8 | 1-2 ms | 1-2 ms | ~1 ms | stays in Alt+Tab and the taskbar; a display change can bring it back into view |
| `DWMWA_CLOAK` | refused for every other app's window | | | | only works on a process's own windows |

Chrome and Edge, showing a page that changes colour four times a second, kept drawing after an
uncloak (5 changes a second, as before hiding). No follow-up nudge (repaint, frame change,
resize, re-raise, re-announced event) changed any result, so none is used. All three methods
keep a maximised window maximised.

## Decision

- Background tabs are hidden with the shell cloak (`IApplicationView::SetCloak`). The interface
  is obtained at run time, checked with `QueryInterface` before use, and every call is verified
  with `DWMWA_CLOAKED`; Explorer restarting is handled by recreating the interface once.
- If the shell cloak is unavailable or fails verification (a future Windows build, Explorer not
  running), Mosaïk falls back to minimising without activation, and says so once.
- Parking is kept for the watchdog and rescue paths (windows parked by an older build), not
  offered as a mode.
- Every hidden window is recorded in the journal and marked with a window property before it is
  hidden, so the watchdog can bring it back whatever happens (ADR 0006).

## Consequences

- An undocumented interface is on the critical path. The checks above turn a broken interface
  into a fallback, never into a call into the wrong method; the probe is re-run before every
  release and on Windows 10 22H2.
- Cloaked windows leave the taskbar and Alt+Tab while hidden, like windows on another virtual
  desktop: a tab group shows as its visible tab.
- UWP windows can fade in after an uncloak if they were minimised before (cosmetic, ~200 ms).
