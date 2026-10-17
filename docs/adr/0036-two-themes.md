# 0036. Two themes: Dark and Light

- Status: accepted
- Date: 2026-10-16
- Amends: ADR 0024 (dark by default, "Like Windows" kept as a choice), 0033 (Like Windows
  follows Windows)

## Context

The owner, on 2026-10-16: "is windows like necessary i feel like its basically light theme ?".
"Like Windows" showed Mosaïk light or dark as Windows' apps were: on his PC, light, so the same
as Light. A third choice that looks like one of the other two is a choice to explain and to test
for nothing most people need; a high-contrast theme of Windows is followed whatever the theme.

## Decision

**Settings offers two themes, Dark (the default) and Light.** Settings saved with "Like Windows"
take, when Mosaïk starts, the theme Windows' apps have then, and are saved so: nothing changes on
screen (`Settings::settle_theme`). A setting sent as "Like Windows" by an older interface is
settled the same way. The window's title bar takes the page's theme and colours, or Windows' own
in a high-contrast theme.

## Consequences

- Core test, failing first: "Like Windows" becomes Light or Dark as Windows is, once; a theme
  chosen stays (core 308).
- Interface tests, failing first: Settings offers Dark and Light only; the title bar takes the
  page's theme (dark or light).
- The UI tour reads the page on screen after Light and after Dark, as light or dark as chosen, in
  place of Like Windows after each.
