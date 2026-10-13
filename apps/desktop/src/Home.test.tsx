import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Home } from './Home';
import type { ArrangePreview, Command, Reply } from './protocol';
import { Feedback } from './ui';

const engine = vi.hoisted(() => ({
  request: vi.fn<(c: Command) => Promise<Reply>>(),
  onWorkspacesChanged: vi.fn<(f: () => void) => () => void>(() => () => undefined),
}));
vi.mock('./engine', () => engine);

afterEach(cleanup);

const WORK = { left: 0, top: 0, right: 1920, bottom: 1032 };

/** The screen now: two windows, and where Arrange (or a layout) would put them. */
function preview(layout: 'auto' | 'thirds', windows = 2): Reply & ArrangePreview {
  const zones =
    layout === 'thirds'
      ? [
          { name: 'Code', color: 'lapis' as const, rect: { left: 0, top: 0, right: 640, bottom: 1032 } },
          { name: 'Browser', color: 'verdigris' as const, rect: { left: 640, top: 0, right: 1280, bottom: 1032 } },
          { name: 'Zone', color: 'saffron' as const, rect: { left: 1280, top: 0, right: 1920, bottom: 1032 } },
        ]
      : [
          { name: 'Code', color: 'lapis' as const, rect: { left: 0, top: 0, right: 960, bottom: 1032 } },
          { name: 'Browser', color: 'verdigris' as const, rect: { left: 960, top: 0, right: 1920, bottom: 1032 } },
        ];
  const all = [
    { label: 'Visual Studio Code', now: { left: 100, top: 100, right: 900, bottom: 800 } },
    { label: 'Google Chrome', now: { left: 600, top: 200, right: 1600, bottom: 900 } },
  ].slice(0, windows);
  return {
    reply: 'preview',
    work_area: WORK,
    zones,
    windows: all.map((w, i) => {
      const z = zones[i];
      return {
        ...w,
        then: z ? z.rect : null,
        minimized: false,
      };
    }),
    elsewhere: 0,
    task: layout === 'auto' ? 'coding' : null,
  };
}

const workspaces = (names: string[], active: string | null = null): Reply => ({
  reply: 'workspaces',
  workspaces: names.map((name, i) => ({
    id: `w${String(i)}`,
    name,
    windows: 2,
    zones: 2,
    hotkey: null,
    apps: ['Visual Studio Code', 'Google Chrome'],
  })),
  active,
});

const onManage = vi.fn();

function page() {
  return render(
    <Feedback>
      <Home shortcut="Win+Alt+Space" onManage={onManage} />
    </Feedback>,
  );
}

/** The engine as most tests want it: answers every command with these. */
function answer(extra: (c: Command) => Reply | null = () => null, list: Reply = workspaces([])) {
  engine.request.mockImplementation((c) => {
    const special = extra(c);
    if (special) return Promise.resolve(special);
    if (c.cmd === 'preview_arrange') return Promise.resolve(preview(c.preset === 'thirds' ? 'thirds' : 'auto'));
    if (c.cmd === 'workspaces') return Promise.resolve(list);
    return Promise.resolve({ reply: 'done', message: 'Done.' });
  });
}

const picture = () => screen.getByRole('img');

describe('the home page', () => {
  beforeEach(() => {
    engine.request.mockReset();
    engine.onWorkspacesChanged.mockClear();
    onManage.mockClear();
  });

  it('shows the main screen as it is now', async () => {
    answer();
    page();
    expect(await screen.findByText('Your main screen now: 2 windows')).toBeTruthy();
    expect(picture().getAttribute('aria-label')).toBe('Your main screen now: Visual Studio Code, Google Chrome.');
    expect(screen.getByRole('heading', { name: 'Tidy your screen', level: 1 })).toBeTruthy();
  });

  it('shows where the windows would go while a layout is pointed at, and the screen again after', async () => {
    answer();
    page();
    await screen.findByText('Your main screen now: 2 windows');
    const thirds = screen.getByRole('button', { name: 'Thirds' });
    fireEvent.pointerEnter(thirds);
    expect(await screen.findByText('With Thirds: every window in its own zone')).toBeTruthy();
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'preview_arrange', preset: 'thirds' });
    expect(picture().getAttribute('aria-label')).toBe('Visual Studio Code in Code, Google Chrome in Browser.');
    expect(picture().dataset.mode).toBe('then');
    fireEvent.pointerLeave(thirds);
    expect(await screen.findByText('Your main screen now: 2 windows')).toBeTruthy();
    expect(picture().dataset.mode).toBe('now');
  });

  it('shows the same to the keyboard: a layout previews when it has focus', async () => {
    answer();
    page();
    await screen.findByText('Your main screen now: 2 windows');
    screen.getByRole('button', { name: 'Thirds' }).focus();
    expect(await screen.findByText('With Thirds: every window in its own zone')).toBeTruthy();
  });

  it('uses a layout on a click, and undoes it', async () => {
    answer((c) => {
      if (c.cmd === 'arrange') return { reply: 'done', message: 'Arranged 2 windows in the “Thirds” layout.' };
      if (c.cmd === 'undo_arrange') return { reply: 'done', message: 'Windows are back where they were.' };
      return null;
    });
    page();
    await screen.findByText('Your main screen now: 2 windows');
    expect(screen.queryByRole('button', { name: 'Undo' })).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Thirds' }));
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'arrange', preset: 'thirds' });
    expect(await screen.findByText('Arranged 2 windows in the “Thirds” layout.')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Undo' }));
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'undo_arrange' });
    expect(await screen.findByText('Windows are back where they were.')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Undo' })).toBeNull();
  });

  it('arranges with Mosaïk’s own choice, made for the task its windows say', async () => {
    answer((c) => (c.cmd === 'arrange' ? { reply: 'done', message: 'Arranged 2 windows for coding.' } : null));
    page();
    await screen.findByText('Your main screen now: 2 windows');
    const arrange = screen.getByRole('button', { name: 'Arrange windows' });
    fireEvent.pointerEnter(arrange);
    expect(await screen.findByText('With Arrange, for coding: every window in its own zone')).toBeTruthy();
    await userEvent.click(arrange);
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'arrange' });
    expect(await screen.findByText('Arranged 2 windows for coding.')).toBeTruthy();
  });

  it('names no task when the windows say none', async () => {
    answer((c) => (c.cmd === 'preview_arrange' && !c.preset ? { ...preview('auto'), task: 'general' } : null));
    page();
    await screen.findByText('Your main screen now: 2 windows');
    fireEvent.pointerEnter(screen.getByRole('button', { name: 'Arrange windows' }));
    expect(await screen.findByText('With Arrange: every window in its own zone')).toBeTruthy();
  });

  it('says why the windows could not be arranged, and offers no undo', async () => {
    answer((c) =>
      c.cmd === 'arrange'
        ? {
            reply: 'error',
            code: 'failed',
            message: 'There are no windows on screen to arrange.',
            hint: 'Open the apps you work with, then arrange them.',
          }
        : null,
    );
    page();
    await screen.findByText('Your main screen now: 2 windows');
    await userEvent.click(screen.getByRole('button', { name: 'Arrange windows' }));
    expect(
      await screen.findByText(
        'There are no windows on screen to arrange. Open the apps you work with, then arrange them.',
      ),
    ).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Undo' })).toBeNull();
  });

  it('says what to do when nothing is open', async () => {
    answer((c) => (c.cmd === 'preview_arrange' ? { ...preview('auto', 0) } : null));
    page();
    expect(await screen.findByText('Open the apps you work with, then come back here.')).toBeTruthy();
    expect(picture().getAttribute('aria-label')).toBe('No windows are open on your main screen.');
  });

  it('says when Mosaïk cannot read the screen', async () => {
    answer((c) =>
      c.cmd === 'preview_arrange'
        ? { reply: 'error', code: 'failed', message: 'Mosaïk is in safe mode: it does not move windows.', hint: null }
        : null,
    );
    page();
    const said = await screen.findByText('Mosaïk is in safe mode: it does not move windows.');
    expect(said.getAttribute('role')).toBe('alert');
    // The picture does not go on saying the screen is being read.
    expect(picture().getAttribute('aria-label')).toBe('Your main screen could not be read.');
    expect(picture().getAttribute('aria-busy')).toBeNull();
    expect(screen.queryByText('Reading your screen…')).toBeNull();
  });

  it('opens a saved workspace, and leads to the page that manages them', async () => {
    answer(
      (c) => (c.cmd === 'switch' ? { reply: 'done', message: 'Opened “Brand”.' } : null),
      workspaces(['Ship', 'Brand'], 'w0'),
    );
    page();
    const ship = (await screen.findByText('Ship')).closest('li');
    if (!ship) throw new Error('no row');
    expect(within(ship).getByText('Open now')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Open “Brand”' }));
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'switch', workspace: 'w1' });
    expect(await screen.findByText('Opened “Brand”.')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Manage workspaces' }));
    expect(onManage).toHaveBeenCalled();
  });

  it('says which shortcut switches workspaces, and nothing more', async () => {
    answer();
    page();
    const line = await screen.findByText(/switches workspaces from anywhere\./);
    expect(line.textContent).toBe('Win plus Alt plus Space switches workspaces from anywhere.');
    expect(screen.queryByText(/quick menu/)).toBeNull();
  });

  it('says how many windows a layout with fewer zones would minimise', async () => {
    answer((c) => {
      if (c.cmd !== 'preview_arrange' || c.preset !== 'full') return null;
      const p = preview('auto');
      const whole: ArrangePreview = {
        work_area: WORK,
        zones: [{ name: 'Zone', color: 'lapis', rect: WORK }],
        windows: p.windows.map((w, i) =>
          i === 0
            ? { ...w, then: { left: 2, top: 2, right: 1918, bottom: 1030 } }
            : { ...w, then: null, minimized: true },
        ),
        elsewhere: 0,
      };
      return { reply: 'preview', ...whole };
    });
    page();
    await screen.findByText('Your main screen now: 2 windows');
    fireEvent.pointerEnter(screen.getByRole('button', { name: 'Whole screen' }));
    expect(await screen.findByText('With Whole screen: 1 window minimized')).toBeTruthy();
    expect(picture().getAttribute('aria-label')).toBe('Visual Studio Code in Zone, Google Chrome minimized.');
  });
  it('shows each workspace’s apps, and the full list to change them', async () => {
    answer(
      (c) => {
        if (c.cmd !== 'apps') return null;
        return {
          reply: 'apps',
          workspace: c.workspace,
          name: 'Ship',
          apps: [
            { id: 's1', label: 'Visual Studio Code', zone: 'z1', tab: false },
            { id: 's2', label: 'Google Chrome', zone: 'z2', tab: false },
          ],
          zones: [
            { id: 'z1', name: 'Code', color: 'lapis' },
            { id: 'z2', name: 'Browser', color: 'verdigris' },
          ],
          windows: [],
        };
      },
      workspaces(['Ship']),
    );
    page();
    const row = (await screen.findByText('Ship')).closest('li');
    if (!row) throw new Error('no row');
    const names = within(row).getByRole('list', { name: 'Its apps' });
    expect(
      within(names)
        .getAllByRole('listitem')
        .map((li) => li.textContent),
    ).toEqual(['Visual Studio Code', 'Google Chrome']);
    const toggle = within(row).getByRole('button', { name: 'Apps of “Ship”' });
    await userEvent.click(toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'apps', workspace: 'w0' });
    expect(await screen.findByRole('combobox', { name: 'Zone of Google Chrome' })).toBeTruthy();
    await userEvent.click(toggle);
    expect(screen.queryByRole('combobox')).toBeNull();
  });
});
