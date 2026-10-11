# 0004. One global hotkey: Win+Alt+Space

- Status: accepted
- Date: 2026-10-11

## Context

Mosaïk opens its switcher from one global hotkey; everything else happens inside the switcher,
the tray menu and the tab strip. A global hotkey is taken from every other app, so it must be
free on a stock Windows, must not collide with typing on common layouts, and must be explained
when it cannot be registered. The brief proposes `Win+Alt+Space` and asks to verify it on US,
Canadian Multilingual Standard, French AZERTY and German QWERTZ.

## Measurements

On the owner's PC (2026-10-11, `docs/STATUS.md`):

- `RegisterHotKey` trials: `Win+Alt+Space`, `Win+Ctrl+Alt+Space`, `Win+Alt+S` and `Win+Alt+Z` are
  free; `Win+Alt+W`, `Win+Ctrl+Space`, `Win+Shift+Space` and `Win+Space` are taken by Windows.
- `mosaik-probe hotkey`: `Win+Alt+Space` registered and fired 2.8 ms after the key press.
  With each installed layout active (English US, French AZERTY and two Canadian layouts),
  Win + **left** Alt + Space fired every time. Win + **right** Alt + Space fired only on the US
  layout: on AltGr layouts the right Alt key is AltGr, which Windows reports as Ctrl+Alt.
  Right Alt + Space alone never fired the hotkey and typed nothing.
- The layout tables (read from `kbdus`, `kbdcan`, `kbdca`, `kbdfr`, `kbdgr`): AltGr types
  characters on 8 (Canadian Multilingual), 14 (Canadian French), 8 (AZERTY) and 9 (German)
  letter and digit keys, none on US; Canadian Multilingual AltGr+Space types a no-break space.

## Decision

- The default switcher hotkey is `Win+Alt+Space` (left Alt; the shortcut recorder shows it that
  way). If it cannot be registered, Mosaïk tries `Win+Alt+S`, `Win+Alt+Z`, then
  `Win+Ctrl+Alt+Space`, uses the first that works, and tells the user which and why.
- Mosaïk never registers bare F-keys globally, nor Ctrl+Alt with a letter or digit (the AltGr
  measurements show those keys type characters on French and German layouts). The shortcut
  recorder refuses them with that reason, and understands AltGr as right Alt plus left Ctrl.
- PowerToys Command Palette also defaults to `Win+Alt+Space`: when registration fails because of
  it, the message names it.
- Per-workspace hotkeys exist but none is bound by default.

## Consequences

- People on AltGr layouts must use the left Alt key for the switcher; the shortcut text says
  "left Alt" where it matters.
- The fallback list is tested on every release with the hotkey probe.
