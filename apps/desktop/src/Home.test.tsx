import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Home } from './Home';
import type { ArrangePreview, Command, Reply } from './protocol';
import {
  WORK,
  coding,
  done,
  failed,
  nothingOpen,
  opening,
  preview,
  said,
  set,
  setsReply,
  shown,
  workspace,
  workspacesReply,
} from './test-fixtures';
import { Announcer } from './ui';

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
const CHANGES: ReadonlySet<Command['cmd']> = new Set(['arrange', 'undo_arrange', 'switch', 'open_set', 'save']);

function answer(auto: ArrangePreview = coding, extra: (c: Command) => Reply | null = () => null) {
  const asked: Command[] = [];
  engine.request.mockImplementation((c) => {
    asked.push(c);
    const reply = extra(c) ?? usual(c);
    if (CHANGES.has(c.cmd) && reply.reply !== 'error') setTimeout(changed, 0);
    return Promise.resolve(reply);
  });
  function usual(c: Command): Reply {
    switch (c.cmd) {
      case 'preview_arrange':
        if (c.preset === 'full') return preview(full);
        if (c.preset === 'quarters') return preview(quarters);
        if (c.preset) return preview(halves);
        return preview(c.moves?.length ? traded : auto);
      case 'preview_workspace':
        return preview(opening);
      case 'arrange':
        return done('Arranged 2 windows for coding.');
      case 'undo_arrange':
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
const arrange = () => screen.getByRole('button', { name: 'Arrange' });
const title = () => screen.getByRole('heading', { level: 2, name: /Your screen|Preview of/ });
/** Opens More layouts, for the five layouts it holds. */
const moreLayouts = () => userEvent.click(screen.getByRole('button', { name: /^More layouts/ }));

describe('Home', () => {
  it('shows the automatic layout for what is open, each window in its zone, and says nothing moves yet', async () => {
    const asked = answer();
    home();
    await waitFor(() => {
      expect(picture()).toHaveAttribute('aria-label', 'Visual Studio Code in zone 1, Google Chrome in zone 2.');
    });
    expect(screen.getByText('Automatic layout for coding. Nothing moves until you press Arrange.')).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Automatic' })).toBeChecked();
    // Nothing else to say: how to swap windows.
    expect(screen.getByText('Drag a window onto another to swap them.')).toBeInTheDocument();
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
    expect(screen.getByText('Halves. Nothing moves until you press Arrange.')).toBeInTheDocument();
    expect(asked.some((c) => c.cmd === 'arrange')).toBe(false);
  });

  it('names the windows a layout with fewer zones would minimise', async () => {
    answer();
    home();
    await screen.findByRole('radio', { name: 'Halves' });
    await moreLayouts();
    await userEvent.click(screen.getByRole('radio', { name: 'Full screen' }));
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
    await screen.findByRole('radio', { name: 'Halves' });
    await moreLayouts();
    await userEvent.click(screen.getByRole('radio', { name: 'Full screen' }));
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
    expect(await shown('Arranged 2 windows for coding.')).toBeInTheDocument();
    expect(said()).toBe('Arranged 2 windows for coding.');
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
    expect(screen.getByRole('button', { name: 'More layouts' })).toBeDisabled();
    expect(picture()).toHaveAttribute('aria-label', 'Nothing is open on your main screen.');
    expect(screen.queryByText('Drag a window onto another to swap them.')).not.toBeInTheDocument();
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
    expect(tile('Visual Studio Code in zone 2')).toHaveStyle({ left: '60%' });
    expect(tile('Google Chrome in zone 1')).toHaveStyle({ left: '0%' });
    await waitFor(() => {
      expect(asked).toContainEqual({ cmd: 'preview_arrange', moves: [{ window: 101, zone: 1 }] });
    });
    expect(tile('Visual Studio Code in zone 2')).toHaveAttribute('aria-pressed', 'false');
    expect(asked.some((c) => c.cmd === 'arrange')).toBe(false);
    await userEvent.click(arrange());
    expect(asked).toContainEqual({ cmd: 'arrange', moves: [{ window: 101, zone: 1 }] });
    expect(await shown('Arranged 2 windows for coding.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Undo swaps' })).not.toBeInTheDocument();
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
    await userEvent.tab();
    expect(tile('Google Chrome in zone 2')).toHaveFocus();
    await userEvent.keyboard('{Enter}');
    await waitFor(() => {
      expect(asked).toContainEqual({ cmd: 'preview_arrange', moves: [{ window: 101, zone: 1 }] });
    });
  });

  it('puts a window in an empty zone of a layout', async () => {
    const asked = answer();
    home();
    await userEvent.click(await screen.findByRole('radio', { name: 'Quarters' }));
    await waitFor(() => {
      expect(picture()).toHaveAttribute('aria-label', 'Visual Studio Code in zone 1, Google Chrome in zone 2.');
    });
    expect(screen.queryByRole('button', { name: 'Empty zone' })).not.toBeInTheDocument();
    await userEvent.click(tile('Visual Studio Code in zone 1'));
    const free = screen.getAllByRole('button', { name: 'Empty zone' });
    expect(free).toHaveLength(2);
    const [, last] = free;
    if (last) await userEvent.click(last);
    await waitFor(() => {
      expect(asked).toContainEqual({ cmd: 'preview_arrange', preset: 'quarters', moves: [{ window: 101, zone: 3 }] });
    });
  });

  it('puts the windows back with Undo swaps, and a layout chosen starts afresh', async () => {
    const asked = answer();
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Visual Studio Code in zone 1' }));
    await userEvent.click(tile('Google Chrome in zone 2'));
    await userEvent.click(await screen.findByRole('button', { name: 'Undo swaps' }));
    expect(screen.queryByRole('button', { name: 'Undo swaps' })).not.toBeInTheDocument();
    await waitFor(() => {
      expect(tile('Visual Studio Code in zone 1')).toHaveStyle({ left: '0%' });
    });
    await userEvent.click(tile('Visual Studio Code in zone 1'));
    await userEvent.click(tile('Google Chrome in zone 2'));
    await userEvent.click(screen.getByRole('radio', { name: 'Halves' }));
    await waitFor(() => {
      expect(picture()).toHaveAttribute('aria-label', 'Visual Studio Code in zone 1, Google Chrome in zone 2.');
    });
    expect(asked.at(-1)).toEqual({ cmd: 'preview_arrange', preset: 'halves' });
    expect(screen.queryByRole('button', { name: 'Undo swaps' })).not.toBeInTheDocument();
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
    expect(screen.queryByRole('button', { name: 'Arrange' })).not.toBeInTheDocument();
    expect(asked.some((c) => c.cmd === 'switch')).toBe(false);
    await userEvent.click(screen.getByRole('button', { name: 'Open “Ship”' }));
    expect(asked).toContainEqual({ cmd: 'switch', workspace: 'ship' });
    expect(await shown('Opened “Ship”; starting 1 app.')).toBeInTheDocument();
    // Open, it is let go: Arrange is back.
    expect(screen.getByRole('button', { name: 'Arrange' })).toBeInTheDocument();
    expect(title()).toHaveTextContent('Your screen');
    expect(screen.getByRole('button', { name: 'Ship' })).toHaveAttribute('aria-pressed', 'false');
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

  it('keeps a workspace shown when its row is clicked again; Back, Esc or a layout show the screen again', async () => {
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
    expect(screen.getByRole('button', { name: 'Arrange' })).toBeInTheDocument();
    // Focus goes back to the row it came from, not lost with the Back button.
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Ship' })).toHaveFocus();
    });
    await userEvent.click(screen.getByRole('button', { name: 'Ship' }));
    expect(await screen.findByRole('button', { name: 'Open “Ship”' })).toBeInTheDocument();
    await userEvent.keyboard('{Escape}');
    expect(screen.getByRole('button', { name: 'Arrange' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Ship' }));
    expect(await screen.findByRole('button', { name: 'Open “Ship”' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('radio', { name: 'Halves' }));
    expect(screen.getByRole('button', { name: 'Arrange' })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Halves' })).toBeChecked();
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

  it('welcomes on the first run, with the side menu where it is, until put away', async () => {
    answer();
    const onWelcomeRead = vi.fn();
    home({ welcome: { sideMenu: 'left' }, onWelcomeRead });
    expect(
      await screen.findByText(
        'Mosaïk tidies your windows. Choose a layout, press Arrange, and Undo puts everything back. The thin line on the left edge of your screen opens the same choices.',
      ),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Got it' }));
    expect(onWelcomeRead).toHaveBeenCalled();
    cleanup();
    home({ welcome: { sideMenu: null }, onWelcomeRead });
    expect(
      await screen.findByText(
        'Mosaïk tidies your windows. Choose a layout, press Arrange, and Undo puts everything back.',
      ),
    ).toBeInTheDocument();
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
    expect(await shown('Arranged 2 windows for coding.')).toBeInTheDocument();
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

  it('offers Automatic and five layouts by name, the other five under More layouts, beside the workspaces', async () => {
    answer();
    home();
    const area = await screen.findByRole('region', { name: 'Your screen' });
    const group = within(area).getByRole('group', { name: 'Layout' });
    const values = () =>
      within(group)
        .getAllByRole('radio')
        .map((r) => r.getAttribute('value'));
    expect(values()).toEqual(['auto', 'halves', 'thirds', 'main-side', 'half-stack', 'quarters']);
    expect(within(group).getByText('Main + side')).toBeVisible();
    await moreLayouts();
    const more = within(group).getByRole('group', { name: 'More layouts' });
    expect(
      within(more)
        .getAllByRole('radio')
        .map((r) => r.getAttribute('value')),
    ).toEqual(['side-main', 'stack-half', 'rows', 'three-rows', 'full']);
    // Chosen there, it shows in More layouts' place, with its name.
    await userEvent.click(within(more).getByRole('radio', { name: 'Three rows' }));
    expect(within(group).queryByRole('group', { name: 'More layouts' })).not.toBeInTheDocument();
    const button = within(group).getByRole('button', { name: 'More layouts: Three rows' });
    expect(button).toHaveTextContent('Three rows');
    expect(
      within(group)
        .getAllByRole('radio')
        .some((r) => (r as HTMLInputElement).checked),
    ).toBe(false);
    expect(screen.getByText('Three rows. Nothing moves until you press Arrange.')).toBeInTheDocument();
    expect(screen.getByRole('region', { name: 'Workspaces' })).toBeInTheDocument();
  });
});
