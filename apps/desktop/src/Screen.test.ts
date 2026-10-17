import { describe, expect, it } from 'vitest';

import type { ArrangePreview, PreviewWindow, Rect } from './protocol';
import { type Dropped, dropped, moved, refusal, zoneIndex } from './Screen';

const LEFT: Rect = { left: 0, top: 0, right: 960, bottom: 1032 };
const RIGHT: Rect = { left: 960, top: 0, right: 1920, bottom: 1032 };

const win = (window: number, label: string, zone: 'a1' | 'a2', app?: string): PreviewWindow => ({
  window,
  label,
  icon: null,
  now: zone === 'a1' ? LEFT : RIGHT,
  then: zone === 'a1' ? LEFT : RIGHT,
  minimized: false,
  zone,
  ...(app === undefined ? {} : { app }),
});

/** Halves: Claude in the left half with MSYS2 stacked behind it, Chrome in the right one. Mosaïk
 * lists each zone's own window first (ADR 0032). */
const halves: ArrangePreview = {
  work_area: { left: 0, top: 0, right: 1920, bottom: 1032 },
  zones: [
    { id: 'a1', name: '1', color: 'lapis', rect: LEFT },
    { id: 'a2', name: '2', color: 'verdigris', rect: RIGHT },
  ],
  windows: [win(1, 'Claude', 'a1', 's1'), win(2, 'Google Chrome', 'a2', 's2'), win(3, 'MSYS2', 'a1', 's3')],
  elsewhere: 0,
  task: null,
};

/** Each zone's windows, front first, and the zones' colours. */
const seen = (p: ArrangePreview) => ({
  zones: p.zones.map((_, z) => p.windows.filter((w) => zoneIndex(p, w) === z).map((w) => w.label)),
  colours: p.zones.map((z) => z.color),
});

describe('the picture, before Mosaïk says', () => {
  it('trades exactly the two windows dropped one onto the other, wherever they are', () => {
    // The review of 598f728: MSYS2, stacked behind Claude, dropped onto Chrome traded with both
    // windows of nothing in particular; Chrome dropped onto MSYS2 traded with Claude.
    const traded = { zones: [['Claude', 'Google Chrome'], ['MSYS2']], colours: ['lapis', 'verdigris'] };
    expect(seen(moved(halves, 3, 1, 2))).toEqual(traded);
    expect(seen(moved(halves, 2, 0, 3))).toEqual(traded);
    // In one zone: MSYS2 comes in front of Claude.
    expect(seen(moved(halves, 3, 0, 1))).toEqual({
      zones: [['MSYS2', 'Claude'], ['Google Chrome']],
      colours: ['lapis', 'verdigris'],
    });
    // Claude onto Chrome: the two own windows trade zones, with their colours; MSYS2 stays,
    // behind Chrome now.
    expect(seen(moved(halves, 1, 1, 2))).toEqual({
      zones: [['Google Chrome', 'MSYS2'], ['Claude']],
      colours: ['verdigris', 'lapis'],
    });
  });

  it('puts a window dropped onto a zone itself behind its windows, every zone keeping its colour', () => {
    // ADR 0033 (the owner: it "permutes with the app of welcoming zone instead of just adding
    // it"): MSYS2, or Claude, onto Chrome's half joins it, behind Chrome; nothing else moves.
    expect(seen(moved(halves, 3, 1))).toEqual({
      zones: [['Claude'], ['Google Chrome', 'MSYS2']],
      colours: ['lapis', 'verdigris'],
    });
    expect(seen(moved(halves, 1, 1))).toEqual({
      zones: [['MSYS2'], ['Google Chrome', 'Claude']],
      colours: ['lapis', 'verdigris'],
    });
    // Onto a zone shown empty: it takes it, with its colour, as before.
    const BOTTOM: Rect = { left: 960, top: 516, right: 1920, bottom: 1032 };
    const three: ArrangePreview = {
      ...halves,
      zones: [...halves.zones, { id: 'a3', name: '3', color: 'saffron', rect: BOTTOM }],
    };
    expect(seen(moved(three, 1, 2))).toEqual({
      zones: [['MSYS2'], ['Google Chrome'], ['Claude']],
      colours: ['saffron', 'verdigris', 'lapis'],
    });
  });

  it('trades two apps of a workspace dropped one onto the other, zones and places front to back', () => {
    const piece = (w: PreviewWindow) => ({
      window: w.window,
      app: w.app ?? null,
      label: w.label,
      icon: null,
      tab: false,
    });
    // Slack stacked behind Chrome; Claude, in front of its zone, dropped onto Slack: Claude takes
    // Slack's place behind Chrome, and Slack takes Claude's, in front of MSYS2.
    const slack = win(4, 'Slack', 'a2', 's4');
    const four: ArrangePreview = { ...halves, windows: [...halves.windows, slack] };
    const [claude] = four.windows;
    if (!claude) throw new Error('the fixture has Claude');
    const d: Dropped = { piece: piece(claude), zone: 1, onto: piece(slack), from: 0 };
    expect(seen(dropped(four, d))).toEqual({
      zones: [
        ['Slack', 'MSYS2'],
        ['Google Chrome', 'Claude'],
      ],
      colours: ['lapis', 'verdigris'],
    });
  });
});

describe('a drop Mosaïk could not make', () => {
  it('is said with its reason, whatever the app', () => {
    // The owner, on 2026-10-17: TFT Academy dragged into another zone on Home's picture "simply
    // doesnt do anything", and no app is an exception. Mosaïk refuses a drop a zone cannot take
    // (ADR 0033) and says which; the picture says why, and what to do instead (a failure always
    // has a way forward).
    const refused: ArrangePreview = {
      ...halves,
      refused: [{ window: 2, zone: 0, least: { width: 1280, height: 720 } }],
    };
    expect(refusal(refused, { window: 2, zone: 0 })).toBe(
      'Google Chrome doesn’t fit zone 1: its app keeps it at least 1280 × 720. Drag a line to make zone 1 bigger, or drop it in a bigger zone.',
    );
    expect(refusal(refused, { window: 1, zone: 1 })).toBeNull();
    expect(refusal(halves, { window: 2, zone: 0 })).toBeNull();
    expect(refusal(refused, undefined)).toBeNull();
  });
});
