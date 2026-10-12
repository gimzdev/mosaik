# 0011. Tags and tab strips in each window's own title bar

- Status: accepted
- Date: 2026-10-12

## Context

Until now every window in a zone sat below a band 28 px high (at 100%) that carried its zone's
tag, or its tab group's strip, and then showed its own title bar: two rows of the screen for one
window's name and controls, on every window. The owner asked for what browsers do: one row,
the app's title bar, carrying the tabs too.

Mosaïk cannot draw inside another app's window, and apps draw their title bars in many ways:
Windows draws classic ones, and browsers, editors, terminals, Office, File Explorer and Store
apps draw their own, with tabs, menus, search boxes and buttons in them. What every app does say
is where its window can be grabbed to move it: Windows asks the window, wherever the pointer goes
(`WM_NCHITTEST`), and the window answers "caption" (`HTCAPTION`) over the empty, draggable
stretches of its title bar.

## Decision

- **Tags and strips go in the window's own title bar, by default.** Mosaïk asks the window the
  same question Windows asks, along one row 15 px (at 100%) below its top edge, every 6 px, and
  takes the stretches that answer "caption" as free (`mosaik_core::titlebar::scan`). Each point
  is asked of the window real input would reach there (the deepest visible, enabled child under
  it, passing to its parent on "transparent"), so other windows covering it do not matter. The
  buttons Windows draws on a classic title bar are taken from Windows
  (`DWMWA_CAPTION_BUTTON_BOUNDS`), not from the window's answers: on Windows 11 a classic
  window answers "caption" over most of its minimise button, which is drawn wider than the
  window's answers assume (job 0062).
- **The tag** goes at the right end of the rightmost stretch with room for the zone's whole name
  (else of the widest with room for 48 px), next to the window's buttons, where title bars rarely
  keep their own controls, centred on the caption's height. It lets clicks through: pressing on
  it grabs the title bar under it. A window dropped anywhere on that stretch joins the window as
  a tab, as it did on the band.
- **A tab group's strip** goes in the widest stretch of its visible tab's title bar with room for
  140 px (the visible tab's chip and the "+N" chip of the others), its chips against the stretch's
  right end. The first 32 px of the stretch stay free, so the window can still be grabbed there.
  The strip takes the mouse only on its chips; between them the title bar works as before. A
  maximised tab group shows its strip in its title bar too.
- **Reading a title bar** asks the app a few hundred times, so it runs on a thread of its own,
  each question given up after 150 ms, and at once for an app that is not responding. A window is
  read when it first needs a tag or strip, and again when its size changes, when its title
  changes (at most once a second), and when it comes to the front, since apps change their title
  bars without telling (a browser that opened a tab). Meanwhile the last reading is used, each
  stretch kept at its distances from the window's left and right edges. That is a guess: it
  places tags and strips until the new reading comes, but moves no window into or out of a band
  (job 0067: Calculator, made the visible tab of a wider group, would have had a 224 px stretch
  for its strip); a reading the app did not answer keeps the stretches, and the size, of the
  last one it did. A reading taken at a size the window no longer has (it moved into a zone or a
  group meanwhile) is taken again 100 ms later, and a window read at a new size is read once
  more 300 ms later, once its app has laid its title bar out again (job 0071: VS Code, read 3 ms
  after a resize, had one free stretch of its two); not while the user moves or sizes it, as the
  end of that reads it anyway. And a window moves into or out of its band only when a second
  reading, taken at least 250 ms after the first asked for it, agrees: an app shown again
  answers differently for a moment (job 0070: Calculator, shown again as a tab, had room for a
  strip in one reading and not in the next, 40 ms later, and moved twice). A change pending
  while the window is hidden or moved is dropped and decided again when it shows.
- **No room, no squeezing.** A window whose title bar has no stretch wide enough (an app that
  draws its own title bar and moves its window itself, a narrow editor full of menus) wears its
  tag or strip above it, as before: it is placed below a band at the top of its zone (`banded`).
  The moves that put windows in zones (opening a workspace, a display change, a drop, a tab shown)
  name the zone they fill, so a banded window lands below its band wherever it goes. Its tag
  goes in that band, also when its app keeps a height that runs into the band (a minimum
  height larger than the zone below the band): the tag then overlaps the window's top-left
  corner rather than leave its zone. Until a window has been read, it shows its ring without a
  tag; a window whose app does not answer keeps no tag until it does, and its group's strip goes
  above it.
- **The band is solid** (2026-10-12, the owner: "if we need 2 rows ... we shouldn't use
  transparent", "should try to one row"): where a tag or strip goes above a window, the band
  is filled with the app's paper colour (#EDEFEA, or #11141B in dark mode) and the zone's ring
  goes around the band and the window together, so the two read as one window with a taller
  title bar, and nothing behind shows through between them. One row stays the rule; the band
  is only for windows whose title bar has no room. A window of a fixed size gets its band right
  above it, wherever it sits in its zone (ADR 0012).
- **The setting** "Tags and tabs: in each window's title bar / above each window" keeps the band
  for everyone who prefers it (`FrameSettings::tags_in_title_bar`, on by default; settings saved
  before keep it on).

## Consequences

- Every window in a zone gets the band's height back: 28 px at 100%, 56 px at 200%.
- Measured (jobs 0062, 0064 and 0067): Notepad, Notepad++, Paint, Edge, Chrome, Explorer,
  Windows Terminal, Word, Excel and Git Bash have room for the tag and the strip. VS Code keeps
  one free stretch left of its command centre: room for the tag, not for a strip, so as a group's
  visible tab it goes below a band. Calculator keeps 40 px of caption beside its buttons (the rest
  of its title bar is its own content): it wears its tag above it.
- A long title can run under the tag in a narrow window: Windows draws classic titles from the
  left and truncates them at the buttons; the tag covers their end, as browsers' tabs do.
- Reading costs about 200 messages to the app, 3 to 30 ms per window measured (job 0070: the
  probe's windows, VS Code, Calculator), on Mosaïk's reading thread, twice per size change (the
  reading once settled). A window that must go into or out of a band does so about a quarter of
  a second after its first reading, once the second agrees.
- A window that is not per-monitor DPI aware is asked in its own scaled coordinates
  (`PhysicalToLogicalPointForPerMonitorDPI`); this is tested at 100% only so far.
- A title bar changed while its window stays in front, without a new title, is read again only
  when the window next changes size, title or comes to the front.
