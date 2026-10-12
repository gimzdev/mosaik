# 0002. The stack: a Rust engine, native switcher and overlays, web windows only on demand

- Status: accepted
- Date: 2026-10-11

## Context

The brief asks for a Rust engine (`windows` crate), a Tauri 2 shell with a React, TypeScript and
Tailwind interface, and native layered windows for the overlays that must keep up with the
pointer. Step 1 was to check that by measurement before building on it. The owner's first rule
since then: minimal, efficient, elegant. Mosaïk runs all day in the tray, so what it costs while
idle matters as much as how fast it reacts.

## Measurements

On the owner's PC (Windows 11 25H2, 1920x1080 at 100%, 75 Hz), `docs/STATUS.md` has the runs.

| What | Result |
| --- | --- |
| Zone preview (layered window) after a pointer move | updated in 1.9 ms (p50), on screen in the next frame (13.3 ms p50 at 75 Hz) |
| Engine loop alone, idle 60 s (hooks, hotkey, scans) | 0.21% of one core, 1.5 MB private |
| App with the switcher as a hidden web window, kept ready | 153.6 MB private, 372 MB working set, six WebView2 processes, ready in 1.5 s |
| App with a native switcher drawn by hardware Direct2D | 51.4 MB private (the graphics driver is loaded into the process for good) |
| App with a native switcher drawn by software Direct2D | **5.2 MB private** (app 4.2 + watchdog 1.0), 29 MB working set, 0.000% CPU over 60 s, ready in 0.4 s |

## Decision

- The engine stays as the brief says: Rust and the `windows` crate, one thread that owns every
  change to other apps' windows.
- The switcher and the overlays (frames, tags, the drag preview) are native windows drawn with
  Direct2D and DirectWrite on a **software** render target. They are small enough to draw on the
  CPU in a few milliseconds, and a hardware target would load the graphics driver into Mosaïk
  for the rest of the day (about 46 MB). The switcher is built around a real list box, so
  screen readers read it like any Windows list.
- Tauri 2 with React, TypeScript and Tailwind stays for the windows people read and fill in
  (settings, the zone editor, the welcome). They are created when opened and destroyed when
  closed: WebView2 costs memory only while one is on screen.
- Native surfaces use the Windows UI font (Segoe UI Variable, or Segoe UI on Windows 10): it is
  already loaded, crisp at tag sizes, and matches the windows the tags sit on. The web windows
  use the brand fonts (Bricolage Grotesque, Instrument Sans, JetBrains Mono). Both use the same
  palette as mosaïk.com.
- Budgets that follow: idle CPU under 1% (measured 0.000%) and **idle memory under 15 MB private**
  for Mosaïk and its watchdog together (measured 5.2 MB), with no web window open. Re-measured
  before every release with job `app-measure` (`docs/STATUS.md`).

## Consequences

- Two ways of drawing interface: native for what must be instant or always there, web for the
  rest. The switcher's look is kept in step with the site by hand (`switcher/paint.rs`).
- Opening Settings starts WebView2 (about 150 MB while it is open); closing it gives that back.
- The switcher's screen-reader path is a list box plus spoken row descriptions; it is checked
  with Narrator in step 6.
