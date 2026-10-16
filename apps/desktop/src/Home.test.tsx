import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Home } from './Home';
import { stretched } from './Screen';
import type { ArrangePreview, Command, Reply } from './protocol';
import {
  WORK,
  appsOf,
  coding,
  done,
  failed,
  nothingOpen,
  opening,
  openingZones,
  preview,
  said,
  set,
  setsReply,
  shown,
  workspace,
  workspacesReply,
  zonesReply,
} from './test-fixtures';
import { Announcer } from './ui';

// jsdom has no pointer capture; the picture holds the pointer on a press.
if (typeof Element.prototype.setPointerCapture !== 'function') {
  Element.prototype.setPointerCapture = () => undefined;
  Element.prototype.releasePointerCapture = () => undefined;
}

const engine = vi.hoisted(() => ({
  request: vi.fn<(c: Command) => Promise<Reply>>(),
  appIcons: vi.fn((keys: string[]) => Promise.resolve(Object.fromEntries(keys.map((k) => [k, null])))),
  onWorkspacesChanged: vi.fn<(callback: () => void) => () => void>(() => () => undefined),
}));
vi.mock('./engine', () => engine);

/** The engine's "workspaces changed" event, to every part of the page that listens for it. */
let listeners: (() => void)[] = [];
const changed = () => {
  for (const l of listeners) l();
};

afterEach(cleanup);
beforeEach(() => {
  engine.request.mockReset();
  listeners = [];
  engine.onWorkspacesChanged.mockImplementation((callback) => {
    listeners.push(callback);
    return () => {
      listeners = listeners.filter((l) => l !== callback);
    };
  });
});

const wait = (ms: number) =>
  new Promise((resolve) => {
    setTimeout(resolve, ms);
  });

/** How many of the commands asked are `cmd`. */
const count = (asked: Command[], cmd: Command['cmd']) => asked.filter((c) => c.cmd === cmd).length;

/** Halves: the editor on the left, the browser on the right. */
const halves: ArrangePreview = {
  ...coding,
  task: null,
  zones: [
    { name: '1', color: 'lapis', rect: { left: 0, top: 0, right: 960, bottom: 1032 } },
    { name: '2', color: 'verdigris', rect: { left: 960, top: 0, right: 1920, bottom: 1032 } },
  ],
  windows: coding.windows.map((w, i) => ({
    ...w,
    then: i === 0 ? { left: 0, top: 0, right: 960, bottom: 1032 } : { left: 960, top: 0, right: 1920, bottom: 1032 },
  })),
};

/** Full screen: one window stays, the other is minimised. */
const full: ArrangePreview = {
  ...coding,
  task: null,
  zones: [{ name: '1', color: 'lapis', rect: WORK }],
  windows: coding.windows.map((w, i) => (i === 0 ? { ...w, then: WORK } : { ...w, then: null, minimized: true })),
};

/** The screen as the automatic layout would leave it already. */
const arranged: ArrangePreview = { ...coding, windows: coding.windows.map((w) => ({ ...w, now: w.then ?? w.now })) };

/** The automatic layout with the editor and the browser traded: each keeps its colour, each zone
 * its number. */
const traded: ArrangePreview = {
  ...coding,
  zones: [
    { name: '1', color: 'verdigris', rect: { left: 0, top: 0, right: 1152, bottom: 1032 } },
    { name: '2', color: 'lapis', rect: { left: 1152, top: 0, right: 1920, bottom: 1032 } },
  ],
  windows: coding.windows.map((w, i) => ({
    ...w,
    then: i === 0 ? { left: 1152, top: 0, right: 1920, bottom: 1032 } : { left: 0, top: 0, right: 1152, bottom: 1032 },
  })),
};

/** Quarters for two windows: the third and fourth quarters are empty. */
const quarters: ArrangePreview = {
  ...coding,
  task: null,
  zones: [
    { name: '1', color: 'lapis', rect: { left: 0, top: 0, right: 960, bottom: 516 } },
    { name: '2', color: 'verdigris', rect: { left: 960, top: 0, right: 1920, bottom: 516 } },
    { name: '3', color: 'saffron', rect: { left: 0, top: 516, right: 960, bottom: 1032 } },
    { name: '4', color: 'madder', rect: { left: 960, top: 516, right: 1920, bottom: 1032 } },
  ],
  windows: coding.windows.map((w, i) => ({
    ...w,
    then: i === 0 ? { left: 0, top: 0, right: 960, bottom: 516 } : { left: 960, top: 0, right: 1920, bottom: 516 },
  })),
};

const ship = workspace({ id: 'ship', name: 'Ship' });
const codingSet = set({
  id: 'coding',
  name: 'Coding',
  apps: ['Visual Studio Code', 'Windows Terminal', 'Google Chrome'],
});

/** Commands after which the engine says the workspaces changed (crates/mosaik-engine/src/thread.rs). */
const CHANGES: ReadonlySet<Command['cmd']> = new Set([
  'arrange',
  'undo_arrange',
  'switch',
  'open_set',
  'save',
  'set_workspace_layout',
  'undo_workspace_layout',
]);

function answer(auto: ArrangePreview = coding, extra: (c: Command) => Reply | null = () => null) {
  const asked: Command[] = [];
  /** Arranged: the windows are where the picture showed them, and Mosaïk keeps them so. */
  let arrangedNow = false;
  let arrangedAs: ArrangePreview | null = null;
  engine.request.mockImplementation((c) => {
    asked.push(c);
    const reply = extra(c) ?? usual(c);
    if (CHANGES.has(c.cmd) && reply.reply !== 'error') setTimeout(changed, 0);
    return Promise.resolve(reply);
  });
  const now = (p: ArrangePreview): ArrangePreview =>
    arrangedNow ? { ...p, windows: p.windows.map((w) => ({ ...w, now: w.then ?? w.now })) } : p;
  /** The zones as resized on the picture, as Mosaïk takes them (an area for each). */
  const resized = (p: ArrangePreview, c: Command) =>
    c.cmd === 'preview_arrange' && c.areas ? stretched(p, c.areas) : p;
  function usual(c: Command): Reply {
    switch (c.cmd) {
      case 'preview_arrange':
        if (c.preset === 'full') return preview(now(resized(full, c)));
        if (c.preset === 'quarters') return preview(now(resized(quarters, c)));
        if (c.preset) return preview(now(resized(halves, c)));
        return preview(now(resized(c.moves?.length ? traded : (arrangedAs ?? auto), c)));
      case 'preview_workspace':
        return preview(opening);
      case 'arrange': {
        arrangedNow = true;
        // Mosaïk keeps the screen as Move windows left it (ADR 0032): its zones, as stretched too.
        const was = c.moves?.length ? traded : (arrangedAs ?? auto);
        arrangedAs = c.areas ? stretched(was, c.areas) : was;
        return done('Moved 2 windows.');
      }
      case 'undo_arrange':
        arrangedNow = false;
        arrangedAs = null;
        return done('Windows are back where they were.');
      case 'switch':
        return done(`Opened “${c.workspace}”.`);
      case 'open_set':
        return done('Opened “Coding”: 2 apps started.');
      case 'workspaces':
        return workspacesReply([ship]);
      case 'sets':
        return setsReply([codingSet]);
      default:
        return done('Done.');
    }
  }
  return asked;
}

function home(props: Partial<Parameters<typeof Home>[0]> = {}) {
  render(
    <Announcer>
      <Home onEditWorkspace={vi.fn()} onEditZones={vi.fn()} {...props} />
    </Announcer>,
  );
}

const picture = () => screen.getByRole('group', { name: /main screen|Visual Studio Code|Nothing is open/ });
const tile = (name: string) => screen.getByRole('button', { name });
const arrange = () => screen.getByRole('button', { name: 'Move windows' });
const title = () => screen.getByRole('heading', { level: 2, name: /Your screen|Preview of/ });
const HINT =
  'Drag a window into a zone to add it there, onto another window to swap them, or drag a line to resize zones.';

describe('Home', () => {
  it('shows the automatic layout for what is open, each window in its zone, and says nothing moves yet', async () => {
    const asked = answer();
    home();
    await waitFor(() => {
      expect(picture()).toHaveAttribute('aria-label', 'Visual Studio Code in zone 1, Google Chrome in zone 2.');
    });
    expect(screen.getByText('Automatic layout for coding. Nothing moves yet.')).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Automatic' })).toBeChecked();
    // Nothing else to say: how to change the picture.
    expect(screen.getByText(HINT)).toBeInTheDocument();
    // Move windows has Mosaïk's tiles; Automatic keeps the spark.
    expect(arrange()).toHaveTextContent('Move windows');
    expect(asked.some((c) => c.cmd === 'arrange')).toBe(false);
  });

  it('shows a layout once it is chosen, and moves nothing yet', async () => {
    const asked = answer();
    home();
    await userEvent.click(await screen.findByRole('radio', { name: 'Halves' }));
    await waitFor(() => {
      expect(picture()).toHaveAttribute('aria-label', 'Visual Studio Code in zone 1, Google Chrome in zone 2.');
    });
    expect(asked).toContainEqual({ cmd: 'preview_arrange', preset: 'halves' });
    expect(screen.getByRole('radio', { name: 'Halves' })).toBeChecked();
    expect(screen.getByText('Halves. Nothing moves yet.')).toBeInTheDocument();
    expect(asked.some((c) => c.cmd === 'arrange')).toBe(false);
  });

  it('names the windows a layout with fewer zones would minimise', async () => {
    answer();
    home();
    await userEvent.click(await screen.findByRole('radio', { name: 'Full screen' }));
    expect(await screen.findByText('Google Chrome will be minimized.')).toBeInTheDocument();
    expect(picture()).toHaveAttribute('aria-label', 'Visual Studio Code in zone 1, Google Chrome minimized.');
  });

  it('counts the windows of one app it would minimise, rather than naming the app twice', async () => {
    // Job 0152: "Steam, File Explorer, File Explorer, Google Chrome and MSYS2 terminal will be
    // minimized."
    const explorer = (window: number) => ({
      window,
      label: 'File Explorer',
      icon: null,
      now: { left: 100, top: 100, right: 900, bottom: 700 },
      then: null,
      minimized: true,
      starting: false,
      tabs: [],
    });
    const two: ArrangePreview = {
      ...full,
      windows: [...(full.windows[0] ? [full.windows[0]] : []), explorer(71), explorer(72), full.windows[1]].filter(
        (w): w is ArrangePreview['windows'][number] => w !== undefined,
      ),
    };
    answer(coding, (c) => (c.cmd === 'preview_arrange' && c.preset === 'full' ? preview(two) : null));
    home();
    await userEvent.click(await screen.findByRole('radio', { name: 'Full screen' }));
    expect(await screen.findByText('2 File Explorer windows and Google Chrome will be minimized.')).toBeInTheDocument();
  });

  it('arranges with the automatic layout, says what it did, and undoes it', async () => {
    const asked = answer();
    home();
    await waitFor(() => {
      expect(arrange()).toBeEnabled();
    });
    await userEvent.click(arrange());
    expect(asked).toContainEqual({ cmd: 'arrange' });
    expect(await shown('Moved 2 windows.')).toBeInTheDocument();
    expect(said()).toBe('Moved 2 windows.');
    await userEvent.click(screen.getByRole('button', { name: 'Undo' }));
    expect(asked).toContainEqual({ cmd: 'undo_arrange' });
    expect(await shown('Windows are back where they were.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();
  });

  it('arranges in the layout chosen', async () => {
    const asked = answer();
    home();
    await userEvent.click(await screen.findByRole('radio', { name: 'Thirds' }));
    await userEvent.click(arrange());
    expect(asked).toContainEqual({ cmd: 'arrange', preset: 'thirds' });
  });

  it('says why the windows could not be arranged, and offers no undo', async () => {
    answer(coding, (c) => (c.cmd === 'arrange' ? failed('Two windows are not responding.') : null));
    home();
    await waitFor(() => {
      expect(arrange()).toBeEnabled();
    });
    await userEvent.click(arrange());
    expect(await screen.findByRole('alert')).toHaveTextContent('Two windows are not responding.');
    expect(screen.queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();
  });

  it('says when the windows are already where the layout puts them', async () => {
    answer(arranged);
    home();
    expect(await screen.findByText('Your windows are already like this.')).toBeInTheDocument();
  });

  it('says what to do when nothing is open, and offers nothing to arrange', async () => {
    answer(nothingOpen);
    home();
    expect(await screen.findByText('Open some apps first.')).toBeInTheDocument();
    expect(arrange()).toBeDisabled();
    expect(screen.getByRole('radio', { name: 'Halves' })).toBeDisabled();
    expect(screen.getByRole('radio', { name: 'Full screen' })).toBeDisabled();
    expect(picture()).toHaveAttribute('aria-label', 'Nothing is open on your main screen.');
    expect(screen.queryByText(HINT)).not.toBeInTheDocument();
    expect(screen.queryByRole('separator')).not.toBeInTheDocument();
  });

  it('says when Mosaïk cannot read the screen', async () => {
    answer(coding, (c) => (c.cmd === 'preview_arrange' ? failed('Safe mode: Mosaïk does not read windows.') : null));
    home();
    expect(await screen.findByRole('alert')).toHaveTextContent('Safe mode: Mosaïk does not read windows.');
    expect(screen.getByRole('group', { name: 'Your main screen could not be read.' })).toBeInTheDocument();
    expect(screen.getByText('Your screen could not be read.')).toBeInTheDocument();
  });

  it('trades two windows picked on the picture, shows it at once, and arranges them so', async () => {
    // The owner: "in this view we should be able to drag drop apps to permute them". A click
    // (or Enter) picks a window; a click on another puts it there.
    const asked = answer();
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Visual Studio Code in zone 1' }));
    expect(tile('Visual Studio Code in zone 1')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByText('Choose where Visual Studio Code goes. Esc cancels.')).toBeInTheDocument();
    await userEvent.click(tile('Google Chrome in zone 2'));
    // At once, then as Mosaïk says: the editor on the right, in zone 2, its colour with it.
    expect(picture()).toHaveAttribute('aria-label', 'Visual Studio Code in zone 2, Google Chrome in zone 1.');
    // Dropped onto the browser: the two trade places, exactly they (ADR 0032).
    const move = { window: 101, zone: 1, onto: 102 };
    await waitFor(() => {
      expect(asked).toContainEqual({ cmd: 'preview_arrange', moves: [move] });
    });
    expect(tile('Visual Studio Code in zone 2')).toHaveAttribute('aria-pressed', 'false');
    expect(screen.getByRole('button', { name: 'Undo changes' })).toBeInTheDocument();
    expect(asked.some((c) => c.cmd === 'arrange')).toBe(false);
    await userEvent.click(arrange());
    expect(asked).toContainEqual({ cmd: 'arrange', moves: [move] });
    expect(await shown('Moved 2 windows.')).toBeInTheDocument();
    // Arranged as they were put, the picture keeps them there, as the screen now is: Mosaïk keeps
    // windows where they are, so the move is let go (sent again, it would trade them back).
    await waitFor(() => {
      expect(asked.filter((c) => c.cmd === 'preview_arrange').at(-1)).toEqual({ cmd: 'preview_arrange' });
    });
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: 'Undo changes' })).not.toBeInTheDocument();
    });
    expect(picture()).toHaveAttribute('aria-label', 'Visual Studio Code in zone 2, Google Chrome in zone 1.');
  });

  it('trades two windows of one zone front to back, shows it at once, and moves them so', async () => {
    // ADR 0032, the owner: "i should be able to permute chrome with claude". Claude, stacked
    // behind the browser in zone 2, put onto it comes to the front.
    const [code, browser] = coding.windows;
    if (!code || !browser) throw new Error('the fixture has two windows');
    const claude = { ...browser, window: 103, label: 'Claude', icon: null };
    const stacked: ArrangePreview = { ...coding, windows: [code, browser, claude] };
    const swapped: ArrangePreview = { ...coding, windows: [code, claude, browser] };
    const asked = answer(stacked, (c) => (c.cmd === 'preview_arrange' && c.moves?.length ? preview(swapped) : null));
    home();
    await waitFor(() => {
      expect(picture()).toHaveAttribute(
        'aria-label',
        'Visual Studio Code in zone 1, Google Chrome in zone 2, Claude in zone 2.',
      );
    });
    await userEvent.click(tile('Claude in zone 2'));
    await userEvent.click(tile('Google Chrome in zone 2'));
    // At once, then as Mosaïk says: Claude in front, both in zone 2.
    expect(picture()).toHaveAttribute(
      'aria-label',
      'Visual Studio Code in zone 1, Claude in zone 2, Google Chrome in zone 2.',
    );
    const move = { window: 103, zone: 1, onto: 102 };
    await waitFor(() => {
      expect(asked).toContainEqual({ cmd: 'preview_arrange', moves: [move] });
    });
    expect(screen.getByRole('button', { name: 'Undo changes' })).toBeInTheDocument();
    // The focus goes with Claude, to its new place.
    await waitFor(() => {
      expect(tile('Claude in zone 2')).toHaveFocus();
    });
    await userEvent.click(arrange());
    expect(asked).toContainEqual({ cmd: 'arrange', moves: [move] });
  });

  it('keeps a trade in a zone to do on a screen already arranged, and does it once', async () => {
    // A trade changes which window is in front, not where windows are: the screen as arranged
    // is not "already like this" while one waits, and Move windows does it once (sent again, it
    // would trade them back).
    const [code, browser] = coding.windows;
    if (!code || !browser) throw new Error('the fixture has two windows');
    const claude = { ...browser, window: 103, label: 'Claude', icon: null };
    const there = (w: typeof code) => ({ ...w, now: w.then ?? w.now });
    const stacked: ArrangePreview = { ...coding, windows: [code, browser, claude].map(there) };
    const swapped: ArrangePreview = { ...coding, windows: [code, claude, browser].map(there) };
    const asked = answer(stacked, (c) => (c.cmd === 'preview_arrange' && c.moves?.length ? preview(swapped) : null));
    home();
    expect(await shown('Your windows are already like this.')).toBeInTheDocument();
    await userEvent.click(tile('Claude in zone 2'));
    await userEvent.click(tile('Google Chrome in zone 2'));
    await waitFor(() => {
      expect(asked).toContainEqual({ cmd: 'preview_arrange', moves: [{ window: 103, zone: 1, onto: 102 }] });
    });
    expect(screen.queryByText('Your windows are already like this.')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Undo changes' })).toBeInTheDocument();
    await userEvent.click(arrange());
    expect(asked).toContainEqual({ cmd: 'arrange', moves: [{ window: 103, zone: 1, onto: 102 }] });
    expect(await shown('Moved 2 windows.')).toBeInTheDocument();
    // Done: the picture is read without it.
    await waitFor(() => {
      expect(asked.filter((c) => c.cmd === 'preview_arrange').at(-1)).toEqual({ cmd: 'preview_arrange' });
    });
    expect(screen.queryByRole('button', { name: 'Undo changes' })).not.toBeInTheDocument();
  });

  it('picks and puts a window with the keyboard alone, and lets go with Escape', async () => {
    const asked = answer();
    home();
    await screen.findByRole('button', { name: 'Visual Studio Code in zone 1' });
    tile('Visual Studio Code in zone 1').focus();
    await userEvent.keyboard('{Enter}');
    expect(screen.getByText('Choose where Visual Studio Code goes. Esc cancels.')).toBeInTheDocument();
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByText('Choose where Visual Studio Code goes. Esc cancels.')).not.toBeInTheDocument();
    expect(tile('Visual Studio Code in zone 1')).toHaveAttribute('aria-pressed', 'false');
    await userEvent.keyboard('{Enter}');
    // The other zones can be chosen now, before the windows in them.
    await userEvent.tab();
    expect(screen.getByRole('button', { name: 'Zone 2' })).toHaveFocus();
    await userEvent.tab();
    expect(tile('Google Chrome in zone 2')).toHaveFocus();
    await userEvent.keyboard('{Enter}');
    await waitFor(() => {
      expect(asked).toContainEqual({ cmd: 'preview_arrange', moves: [{ window: 101, zone: 1, onto: 102 }] });
    });
    // The focus stays with the window put there.
    expect(tile('Visual Studio Code in zone 2')).toHaveFocus();
  });

  it('adds a window put in a zone with windows behind them, and moves no other', async () => {
    // ADR 0033 (the owner: a window dragged into another zone "permutes with the app of welcoming
    // zone instead of just adding it"). Visual Studio Code put in Chrome's zone itself joins it,
    // behind Chrome; Chrome stays.
    const [code, browser] = coding.windows;
    const right = coding.zones[1]?.rect;
    if (!code || !browser || !right) throw new Error('the fixture has two windows in two zones');
    const joined: ArrangePreview = { ...coding, windows: [browser, { ...code, then: right }] };
    const asked = answer(coding, (c) =>
      c.cmd === 'preview_arrange' && c.moves?.length && c.moves.every((m) => m.onto === undefined)
        ? preview(joined)
        : null,
    );
    // Mosaïk answers the move only once the picture has been read before its answer.
    const usual = engine.request.getMockImplementation();
    if (!usual) throw new Error('the stand-in engine answers');
    let release: () => void = () => undefined;
    engine.request.mockImplementation((c) =>
      c.cmd === 'preview_arrange' && c.moves?.length
        ? new Promise<Reply>((resolve) => {
            release = () => {
              void usual(c).then(resolve);
            };
          })
        : usual(c),
    );
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Visual Studio Code in zone 1' }));
    await userEvent.click(screen.getByRole('button', { name: 'Zone 2' }));
    const label = 'Google Chrome in zone 2, Visual Studio Code in zone 2. Zone 1 is empty.';
    // At once, then as Mosaïk says.
    expect(picture()).toHaveAttribute('aria-label', label);
    release();
    await waitFor(() => {
      expect(asked).toContainEqual({ cmd: 'preview_arrange', moves: [{ window: 101, zone: 1 }] });
    });
    await wait(50);
    expect(picture()).toHaveAttribute('aria-label', label);
  });

  it('puts a window in an empty zone of a layout', async () => {
    const asked = answer();
    home();
    await userEvent.click(await screen.findByRole('radio', { name: 'Quarters' }));
    // An empty zone has its number and colour, and says it is empty.
    await waitFor(() => {
      expect(picture()).toHaveAttribute(
        'aria-label',
        'Visual Studio Code in zone 1, Google Chrome in zone 2. Zones 3 and 4 are empty.',
      );
    });
    expect(screen.getAllByText('Empty')).toHaveLength(2);
    expect(screen.queryByRole('button', { name: 'Zone 4, empty' })).not.toBeInTheDocument();
    await userEvent.click(tile('Visual Studio Code in zone 1'));
    expect(screen.getByRole('button', { name: 'Zone 3, empty' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Zone 4, empty' }));
    await waitFor(() => {
      expect(asked).toContainEqual({ cmd: 'preview_arrange', preset: 'quarters', moves: [{ window: 101, zone: 3 }] });
    });
  });

  it('puts the windows back with Undo changes, and a layout chosen starts afresh', async () => {
    const asked = answer();
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Visual Studio Code in zone 1' }));
    await userEvent.click(tile('Google Chrome in zone 2'));
    await userEvent.click(await screen.findByRole('button', { name: 'Undo changes' }));
    expect(screen.queryByRole('button', { name: 'Undo changes' })).not.toBeInTheDocument();
    await waitFor(() => {
      expect(picture()).toHaveAttribute('aria-label', 'Visual Studio Code in zone 1, Google Chrome in zone 2.');
    });
    await userEvent.click(tile('Visual Studio Code in zone 1'));
    await userEvent.click(tile('Google Chrome in zone 2'));
    await userEvent.click(screen.getByRole('radio', { name: 'Halves' }));
    await waitFor(() => {
      expect(picture()).toHaveAttribute('aria-label', 'Visual Studio Code in zone 1, Google Chrome in zone 2.');
    });
    expect(asked.at(-1)).toEqual({ cmd: 'preview_arrange', preset: 'halves' });
    expect(screen.queryByRole('button', { name: 'Undo changes' })).not.toBeInTheDocument();
  });

  it('resizes the zones by the line between them, shows it at once, and arranges them so', async () => {
    // The owner: "for anim homescreen we should be able to stretch zones".
    const asked = answer();
    home();
    const line = await screen.findByRole('separator', { name: 'Line between zone 1 and zone 2' });
    expect(line).toHaveAttribute('aria-valuenow', '60');
    line.focus();
    await userEvent.keyboard('{ArrowLeft}');
    // A 48th of the screen to the left, on the grid: 28/48.
    const areas = [
      { left: 0, top: 0, right: 28 / 48, bottom: 1 },
      { left: 28 / 48, top: 0, right: 1, bottom: 1 },
    ];
    expect(screen.getByRole('separator', { name: 'Line between zone 1 and zone 2' })).toHaveAttribute(
      'aria-valuenow',
      '58',
    );
    await waitFor(() => {
      expect(asked).toContainEqual({ cmd: 'preview_arrange', areas });
    });
    expect(screen.getByRole('button', { name: 'Undo changes' })).toBeInTheDocument();
    await userEvent.click(arrange());
    expect(asked).toContainEqual({ cmd: 'arrange', areas });
    expect(await shown('Moved 2 windows.')).toBeInTheDocument();
    // Done: the screen holds them now, and the picture is read as it is, stretched.
    await waitFor(() => {
      expect(asked.filter((c) => c.cmd === 'preview_arrange').at(-1)).toEqual({ cmd: 'preview_arrange' });
    });
    expect(screen.queryByRole('button', { name: 'Undo changes' })).not.toBeInTheDocument();
    expect(screen.getByRole('separator', { name: 'Line between zone 1 and zone 2' })).toHaveAttribute(
      'aria-valuenow',
      '58',
    );
  });

  it('lets go of zones resized once the screen has other zones', async () => {
    const asked = answer();
    home();
    const line = await screen.findByRole('separator', { name: 'Line between zone 1 and zone 2' });
    line.focus();
    await userEvent.keyboard('{ArrowLeft}');
    await waitFor(() => {
      expect(asked.some((c) => c.cmd === 'preview_arrange' && c.areas !== undefined)).toBe(true);
    });
    // A third window came: Mosaïk lays out three zones, which the two areas do not fit.
    answer(quarters);
    window.dispatchEvent(new Event('focus'));
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: 'Undo changes' })).not.toBeInTheDocument();
    });
  });

  it('shows a workspace picked as a preview of how it opens, and opens it with Open', async () => {
    // The owner: "we shouldnt open the workspace when clicking on it it should show it in the
    // anim as preview and show an open button same as when you edit the workspace".
    const asked = answer(coding, (c) => (c.cmd === 'switch' ? done('Opened “Ship”; starting 1 app.') : null));
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Ship' }));
    expect(asked).toContainEqual({ cmd: 'preview_workspace', workspace: 'ship' });
    await waitFor(() => {
      expect(picture()).toHaveAttribute(
        'aria-label',
        'Visual Studio Code in zone 1, Windows Terminal starts in zone 2 with Claude as tabs, Google Chrome minimized.',
      );
    });
    expect(title()).toHaveTextContent('Preview of “Ship”');
    expect(screen.getByText('Nothing moves until you press Open.')).toBeInTheDocument();
    expect(screen.getByText('Windows Terminal will start. Google Chrome will be minimized.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Ship' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.queryByRole('button', { name: 'Move windows' })).not.toBeInTheDocument();
    expect(asked.some((c) => c.cmd === 'switch')).toBe(false);
    await userEvent.click(screen.getByRole('button', { name: 'Open “Ship”' }));
    expect(asked).toContainEqual({ cmd: 'switch', workspace: 'ship' });
    expect(await shown('Opened “Ship”; starting 1 app.')).toBeInTheDocument();
    // Open, it is let go: Move windows is back.
    expect(arrange()).toBeInTheDocument();
    expect(title()).toHaveTextContent('Your screen');
    expect(screen.getByRole('button', { name: 'Ship' })).toHaveAttribute('aria-pressed', 'false');
  });

  it('lays out a workspace picked in the layout chosen, keeping its apps, and undoes it', async () => {
    // The owner: "try coding for example theres 6 apps normally and going to quarter get rid of
    // some". A layout chosen while a workspace was shown went back to the screen, which has
    // fewer windows. It lays out that workspace, saved at once (ADR 0033), and Undo puts it back.
    const asked = answer(coding, (c) =>
      c.cmd === 'set_workspace_layout'
        ? done('“Ship” is laid out in Quarters.')
        : c.cmd === 'undo_workspace_layout'
          ? done('“Ship” is back as it was.')
          : null,
    );
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Ship' }));
    await waitFor(() => {
      expect(picture()).toHaveAttribute(
        'aria-label',
        'Visual Studio Code in zone 1, Windows Terminal starts in zone 2 with Claude as tabs, Google Chrome minimized.',
      );
    });
    // Automatic lays out the screen's windows; it is no layout of a workspace.
    expect(screen.getByRole('radio', { name: 'Automatic' })).toBeDisabled();
    await userEvent.click(screen.getByRole('radio', { name: 'Quarters' }));
    expect(asked).toContainEqual({ cmd: 'set_workspace_layout', workspace: 'ship', preset: 'quarters' });
    expect(await shown('“Ship” is laid out in Quarters.')).toBeInTheDocument();
    // Still the workspace's preview, read again; not the screen's.
    expect(title()).toHaveTextContent('Preview of “Ship”');
    expect(asked.some((c) => c.cmd === 'preview_arrange' && c.preset === 'quarters')).toBe(false);
    await waitFor(() => {
      expect(count(asked, 'preview_workspace')).toBeGreaterThan(1);
    });
    await userEvent.click(screen.getByRole('button', { name: 'Undo' }));
    expect(asked).toContainEqual({ cmd: 'undo_workspace_layout', workspace: 'ship' });
    expect(await shown('“Ship” is back as it was.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();
  });

  it('changes a workspace picked on its picture, saving it there at once', async () => {
    // The owner: "when clicking on a workspace the anim must have those options too (drag drop
    // and stretch zones)".
    const asked = answer(coding, (c) =>
      c.cmd === 'move_app' ? appsOf() : c.cmd === 'zones' || c.cmd === 'set_zones' ? zonesReply(openingZones) : null,
    );
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Ship' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Visual Studio Code in zone 1' }));
    await userEvent.click(screen.getByRole('button', { name: 'Zone 2' }));
    expect(asked).toContainEqual({ cmd: 'move_app', workspace: 'ship', app: 's1', zone: 'z2' });
    expect(await shown('Visual Studio Code goes in zone 2 of “Ship”.')).toBeInTheDocument();
    // Dropped on a tab of a group, the two trade zones; the tab leaves its group.
    await userEvent.click(await screen.findByRole('button', { name: 'Visual Studio Code in zone 1' }));
    await userEvent.click(screen.getByRole('button', { name: 'Claude, a tab in zone 2' }));
    expect(asked).toContainEqual({ cmd: 'move_app', workspace: 'ship', app: 's1', zone: 'z2', swap_with: 's3' });
    expect(await shown('Visual Studio Code and Claude traded zones in “Ship”.')).toBeInTheDocument();
    // Its zones resized by the line between them: only the edges moved change.
    const line = screen.getByRole('separator', { name: 'Line between zone 1 and zone 2' });
    line.focus();
    await userEvent.keyboard('{ArrowRight}');
    expect(await shown('Zones of “Ship” resized.')).toBeInTheDocument();
    const [z1, z2, other] = openingZones;
    expect(asked).toContainEqual({
      cmd: 'set_zones',
      workspace: 'ship',
      revision: '7',
      zones: [
        { ...z1, area: { left: 0, top: 0, right: 30 / 48, bottom: 1 } },
        { ...z2, area: { left: 30 / 48, top: 0, right: 1, bottom: 1 } },
        other,
      ],
    });
    expect(asked.some((c) => c.cmd === 'switch')).toBe(false);
    // Saved in the workspace, what opening it did can no longer be undone.
    expect(screen.queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();
  });

  it('lets go of a window picked once the picture cannot change, for good', async () => {
    // Review of 596ba4f: picked, then Move windows, the window came back picked, unsaid, with
    // the zones' buttons over the picture and its lines hidden.
    answer();
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Visual Studio Code in zone 1' }));
    expect(screen.getByText('Choose where Visual Studio Code goes. Esc cancels.')).toBeInTheDocument();
    await userEvent.click(arrange());
    expect(await shown('Moved 2 windows.')).toBeInTheDocument();
    await waitFor(() => {
      expect(tile('Visual Studio Code in zone 1')).toBeInTheDocument();
    });
    expect(tile('Visual Studio Code in zone 1')).toHaveAttribute('aria-pressed', 'false');
    expect(screen.queryByRole('button', { name: 'Zone 2' })).not.toBeInTheDocument();
    expect(screen.getByRole('separator', { name: 'Line between zone 1 and zone 2' })).toBeInTheDocument();
    expect(screen.queryByText(/Choose where/)).not.toBeInTheDocument();
  });

  it('saves nothing for a line let go of with Esc, even on a click on a line after', async () => {
    // Review of 596ba4f: the zones as they were when the drag was cancelled were saved in the
    // workspace shown next, on a click on its line that moved nothing.
    const asked = answer(coding, (c) => (c.cmd === 'zones' || c.cmd === 'set_zones' ? zonesReply(openingZones) : null));
    home();
    await userEvent.click(await screen.findByRole('radio', { name: 'Halves' }));
    await waitFor(() => {
      expect(screen.getByRole('separator', { name: 'Line between zone 1 and zone 2' })).toHaveAttribute(
        'aria-valuenow',
        '50',
      );
    });
    const line = screen.getByRole('separator', { name: 'Line between zone 1 and zone 2' });
    fireEvent.pointerDown(line, { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
    fireEvent.keyDown(line, { key: 'Escape' });
    fireEvent.pointerUp(line, { button: 0, pointerId: 1 });
    await userEvent.click(await screen.findByRole('button', { name: 'Ship' }));
    await waitFor(() => {
      expect(screen.getByRole('separator', { name: 'Line between zone 1 and zone 2' })).toHaveAttribute(
        'aria-valuenow',
        '60',
      );
    });
    const other = screen.getByRole('separator', { name: 'Line between zone 1 and zone 2' });
    fireEvent.pointerDown(other, { button: 0, pointerId: 2, clientX: 10, clientY: 10 });
    fireEvent.pointerUp(other, { button: 0, pointerId: 2 });
    await wait(300);
    expect(asked.some((c) => c.cmd === 'set_zones' || c.cmd === 'zones')).toBe(false);
    expect(screen.queryByText(/resized/)).not.toBeInTheDocument();
  });

  it('keeps the focus on a tab put in another zone with the keyboard, once Mosaïk has moved it', async () => {
    // Review of 596ba4f: the tab moved, as a window of its own, and the focus fell to the page.
    let moved = false;
    const [code, terminal, chrome] = opening.windows;
    if (!code || !terminal || !chrome) throw new Error('the fixture has three windows');
    const after: ArrangePreview = {
      ...opening,
      windows: [
        code,
        { ...terminal, tabs: [] },
        {
          window: 0,
          label: 'Claude',
          icon: 'app:Claude',
          now: { left: 0, top: 0, right: 1152, bottom: 1032 },
          then: { left: 0, top: 0, right: 1152, bottom: 1032 },
          minimized: false,
          starting: true,
          zone: 'z1',
          app: 's3',
        },
        chrome,
      ],
    };
    answer(coding, (c) => {
      if (c.cmd === 'move_app') {
        moved = true;
        return appsOf();
      }
      return c.cmd === 'preview_workspace' ? preview(moved ? after : opening) : null;
    });
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Ship' }));
    screen.getByRole('button', { name: 'Claude, a tab in zone 2' }).focus();
    await userEvent.keyboard('{Enter}');
    screen.getByRole('button', { name: 'Zone 1' }).focus();
    await userEvent.keyboard('{Enter}');
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Claude starts in zone 1' })).toHaveFocus();
    });
  });

  it('ends a drag whose window is gone from the picture, and drags nothing after', async () => {
    // Review of 596ba4f: the window dragged left the picture (it closed), the pointer's release
    // went elsewhere, and the drag stayed under way with its ghost.
    let gone = false;
    const [code, browser] = coding.windows;
    if (!code || !browser) throw new Error('the fixture has two windows');
    const asked = answer(coding, (c) =>
      c.cmd === 'preview_arrange' && gone ? preview({ ...coding, windows: [browser] }) : null,
    );
    home();
    const bar = await screen.findByRole('button', { name: 'Visual Studio Code in zone 1' });
    fireEvent.pointerDown(bar, { button: 0, pointerId: 1, clientX: 20, clientY: 20 });
    fireEvent.pointerMove(bar, { pointerId: 1, clientX: 60, clientY: 60 });
    // Its ghost, under the pointer.
    expect(screen.getAllByText('Visual Studio Code')).toHaveLength(2);
    gone = true;
    window.dispatchEvent(new Event('focus'));
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: 'Visual Studio Code in zone 1' })).not.toBeInTheDocument();
    });
    fireEvent.pointerUp(window, { pointerId: 1 });
    expect(screen.queryByText('Visual Studio Code')).not.toBeInTheDocument();
    const chrome = screen.getByRole('button', { name: 'Google Chrome in zone 2' });
    fireEvent.pointerDown(chrome, { button: 0, pointerId: 2, clientX: 20, clientY: 20 });
    fireEvent.pointerUp(chrome, { pointerId: 2 });
    expect(asked.filter((c) => c.cmd === 'preview_arrange' && c.moves !== undefined)).toEqual([]);
  });

  it('says why a workspace could not be changed on its picture, and shows it as it is', async () => {
    const asked = answer(coding, (c) => (c.cmd === 'move_app' ? failed('The workspace could not be saved.') : null));
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Ship' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Visual Studio Code in zone 1' }));
    await userEvent.click(screen.getByRole('button', { name: 'Zone 2' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The workspace could not be saved.');
    await waitFor(() => {
      expect(count(asked, 'preview_workspace')).toBeGreaterThan(1);
    });
    expect(picture()).toHaveAttribute(
      'aria-label',
      'Visual Studio Code in zone 1, Windows Terminal starts in zone 2 with Claude as tabs, Google Chrome minimized.',
    );
  });

  it('undoes opening a workspace: the windows it minimised come back', async () => {
    // Opening a workspace minimises the other windows on its screens; a person new to Mosaïk
    // would otherwise bring them back one by one (ADR 0029). A set started is not undone.
    const asked = answer();
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Ship' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Open “Ship”' }));
    expect(await shown('Opened “ship”.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Undo' }));
    expect(asked).toContainEqual({ cmd: 'undo_arrange' });
    expect(await shown('Windows are back where they were.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();
  });

  it('keeps a workspace shown when its row is clicked again, and when a layout is chosen; Back or Esc show the screen again', async () => {
    const asked = answer();
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Ship' }));
    expect(await screen.findByRole('button', { name: 'Open “Ship”' })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Automatic' })).not.toBeChecked();
    // A second click does not let it go.
    await userEvent.click(screen.getByRole('button', { name: 'Ship' }));
    expect(screen.getByRole('button', { name: 'Open “Ship”' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Back to my screen' }));
    expect(title()).toHaveTextContent('Your screen');
    expect(arrange()).toBeInTheDocument();
    // Focus goes back to the row it came from, not lost with the Back button.
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Ship' })).toHaveFocus();
    });
    await userEvent.click(screen.getByRole('button', { name: 'Ship' }));
    expect(await screen.findByRole('button', { name: 'Open “Ship”' })).toBeInTheDocument();
    await userEvent.keyboard('{Escape}');
    expect(arrange()).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Ship' }));
    expect(await screen.findByRole('button', { name: 'Open “Ship”' })).toBeInTheDocument();
    // A layout chosen is the workspace's own now (ADR 0033): it stays shown.
    await userEvent.click(screen.getByRole('radio', { name: 'Halves' }));
    expect(asked).toContainEqual({ cmd: 'set_workspace_layout', workspace: 'ship', preset: 'halves' });
    expect(screen.getByRole('button', { name: 'Open “Ship”' })).toBeInTheDocument();
    expect(asked.some((c) => c.cmd === 'switch')).toBe(false);
  });

  it('opens a workspace at once with a double-click on its row', async () => {
    const asked = answer();
    home();
    await userEvent.dblClick(await screen.findByRole('button', { name: 'Ship' }));
    expect(asked).toContainEqual({ cmd: 'switch', workspace: 'ship' });
    expect(await shown('Opened “ship”.')).toBeInTheDocument();
    expect(title()).toHaveTextContent('Your screen');
  });

  it('says what the column’s actions did under the picture, in one place', async () => {
    answer(coding, (c) => (c.cmd === 'switch' ? failed('Its monitor is gone.', 'Edit its zones.') : null));
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Open' }));
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Its monitor is gone. Edit its zones.');
    expect(within(screen.getByRole('region', { name: 'Your screen' })).getByRole('alert')).toBe(alert);
  });

  it('shows what a suggested set starts, and starts it only with Start these apps', async () => {
    const asked = answer();
    home();
    await userEvent.click(await screen.findByRole('button', { name: /^Coding \(suggested\)/ }));
    expect(title()).toHaveTextContent('Preview of “Coding”');
    expect(screen.getByText('Nothing starts until you press Start these apps.')).toBeInTheDocument();
    expect(
      screen.getByText(
        'Coding starts Visual Studio Code, Windows Terminal and Google Chrome, and puts them in their zones.',
      ),
    ).toBeInTheDocument();
    const apps = screen.getByRole('list', { name: 'Apps of “Coding”' });
    expect(
      within(apps)
        .getAllByRole('listitem')
        .map((a) => a.textContent),
    ).toEqual(['Visual Studio Code', 'Windows Terminal', 'Google Chrome']);
    expect(asked.some((c) => c.cmd === 'open_set')).toBe(false);
    await userEvent.click(screen.getByRole('button', { name: 'Start these apps' }));
    expect(asked).toContainEqual({ cmd: 'open_set', set: 'coding' });
    expect(await shown('Opened “Coding”: 2 apps started.')).toBeInTheDocument();
    expect(title()).toHaveTextContent('Your screen');
  });

  it('says which of a suggested set’s apps are open already, by how many', async () => {
    answer(coding, (c) => (c.cmd === 'sets' ? setsReply([{ ...codingSet, open: 1 }]) : null));
    home();
    await userEvent.click(await screen.findByRole('button', { name: /^Coding \(suggested\)/ }));
    expect(
      screen.getByText(
        'Coding puts Visual Studio Code, Windows Terminal and Google Chrome in their zones, starting the 2 not open yet.',
      ),
    ).toBeInTheDocument();
  });

  it('does not promise to start apps when Mosaïk does not start closed apps', async () => {
    const asked = answer();
    home({ startsApps: false });
    await userEvent.click(await screen.findByRole('button', { name: /^Coding \(suggested\)/ }));
    expect(
      screen.getByText(
        'Coding puts Visual Studio Code, Windows Terminal and Google Chrome in their zones, if they are open.',
      ),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Open “Coding”' }));
    expect(asked).toContainEqual({ cmd: 'open_set', set: 'coding' });
  });

  it('reads the screen once for a burst of focus events, and once after Arrange', async () => {
    // Every read is a scan of every window in the engine, served one at a time: a burst of them
    // holds up drags and shortcuts.
    const asked = answer();
    home();
    await waitFor(() => {
      expect(arrange()).toBeEnabled();
    });
    await wait(300);
    const first = count(asked, 'preview_arrange');
    for (let i = 0; i < 3; i += 1) {
      window.dispatchEvent(new Event('focus'));
      await wait(40);
    }
    await wait(400);
    expect(count(asked, 'preview_arrange') - first).toBe(1);
    const second = count(asked, 'preview_arrange');
    await userEvent.click(arrange());
    expect(await shown('Moved 2 windows.')).toBeInTheDocument();
    await wait(400);
    expect(count(asked, 'preview_arrange') - second).toBe(1);
  });

  it('reads only the picture shown: a workspace’s, not the screen’s, while it is shown', async () => {
    const asked = answer();
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Ship' }));
    await screen.findByText('Windows Terminal will start. Google Chrome will be minimized.');
    await wait(300);
    const before = { screen: count(asked, 'preview_arrange'), workspace: count(asked, 'preview_workspace') };
    for (let i = 0; i < 3; i += 1) {
      window.dispatchEvent(new Event('focus'));
      await wait(40);
    }
    await wait(400);
    expect(count(asked, 'preview_arrange') - before.screen).toBe(0);
    expect(count(asked, 'preview_workspace') - before.workspace).toBe(1);
  });

  it('offers Automatic and the eight layouts in two rows, all in view, beside the workspaces', async () => {
    // The owner: "we dont want dropdown menu", "remove the ones that are only rows", "9 left
    // fitting in 2 lines of options".
    answer();
    home();
    const area = await screen.findByRole('region', { name: 'Your screen' });
    const group = within(area).getByRole('group', { name: 'Layout' });
    expect(
      within(group)
        .getAllByRole('radio')
        .map((r) => r.getAttribute('value')),
    ).toEqual(['auto', 'halves', 'thirds', 'main-side', 'side-main', 'half-stack', 'stack-half', 'quarters', 'full']);
    expect(within(group).queryByRole('button')).not.toBeInTheDocument();
    expect(within(group).queryByText('Top + bottom')).not.toBeInTheDocument();
    expect(within(group).queryByText('Three rows')).not.toBeInTheDocument();
    await userEvent.click(within(group).getByRole('radio', { name: 'Stack + half' }));
    expect(within(group).getByRole('radio', { name: 'Stack + half' })).toBeChecked();
    expect(screen.getByText('Stack + half. Nothing moves yet.')).toBeInTheDocument();
    expect(screen.getByRole('region', { name: 'Workspaces' })).toBeInTheDocument();
  });
});
