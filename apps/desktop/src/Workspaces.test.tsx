import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Command, Reply, WorkspaceApps } from './protocol';
import { Feedback } from './ui';
import { Workspaces } from './Workspaces';

const engine = vi.hoisted(() => ({
  request: vi.fn<(c: Command) => Promise<Reply>>(),
  onWorkspacesChanged: vi.fn<(f: () => void) => () => void>(() => () => undefined),
}));
vi.mock('./engine', () => engine);

afterEach(cleanup);

const list = (names: string[]): Reply => ({
  reply: 'workspaces',
  workspaces: names.map((name, i) => ({
    id: `w${String(i)}`,
    name,
    windows: 3,
    zones: 1,
    hotkey: i === 0 ? 'Win+Alt+J' : null,
    apps: ['Visual Studio Code', 'Google Chrome', 'Windows Terminal'],
  })),
  active: 'w0',
});

const onHome = vi.fn();

function page() {
  return render(
    <Feedback>
      <Workspaces onEditZones={vi.fn()} onHome={onHome} />
    </Feedback>,
  );
}

describe('the workspaces page', () => {
  beforeEach(() => {
    engine.request.mockReset();
    engine.onWorkspacesChanged.mockClear();
    onHome.mockClear();
  });

  it('explains how to make one when there is none, and leads to Arrange', async () => {
    engine.request.mockResolvedValue(list([]));
    page();
    expect(await screen.findByRole('heading', { name: 'No workspaces yet' })).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Arrange windows' }));
    expect(onHome).toHaveBeenCalled();
  });

  it('opens the switcher', async () => {
    engine.request.mockImplementation((c) =>
      Promise.resolve(c.cmd === 'show_switcher' ? { reply: 'done', message: 'The switcher is open.' } : list(['Ship'])),
    );
    page();
    await screen.findAllByRole('listitem');
    await userEvent.click(screen.getByRole('button', { name: 'Open the switcher' }));
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'show_switcher' });
  });

  it('lists workspaces with what they hold and opens one', async () => {
    engine.request.mockImplementation((c) =>
      Promise.resolve(c.cmd === 'switch' ? { reply: 'done', message: 'Opened “Brand”.' } : list(['Ship', 'Brand'])),
    );
    page();
    const rows = await screen.findAllByRole('listitem');
    expect(rows).toHaveLength(2);
    const first = rows[0];
    if (!first) throw new Error('no rows');
    expect(within(first).getByText('Open now')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Open “Brand”' }));
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'switch', workspace: 'w1' });
    expect(await screen.findByText('Opened “Brand”.')).toBeTruthy();
  });

  it('reads them again when they change elsewhere', async () => {
    engine.request.mockResolvedValue(list(['Ship']));
    page();
    await screen.findAllByRole('listitem');
    const changed = engine.onWorkspacesChanged.mock.calls[0]?.[0];
    if (!changed) throw new Error('the page does not listen for changes');
    engine.request.mockResolvedValue(list(['Ship', 'Saved from the switcher']));
    changed();
    expect(await screen.findByText('Saved from the switcher')).toBeTruthy();
  });

  it('asks before deleting', async () => {
    engine.request.mockImplementation((c) =>
      Promise.resolve(c.cmd === 'delete_workspace' ? { reply: 'done', message: 'Deleted “Ship”.' } : list(['Ship'])),
    );
    page();
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Delete…' }));
    expect(engine.request).not.toHaveBeenCalledWith({ cmd: 'delete_workspace', workspace: 'w0' });
    await userEvent.click(screen.getByRole('button', { name: 'Delete workspace', hidden: true }));
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'delete_workspace', workspace: 'w0' });
  });

  it('shows why a rename was refused', async () => {
    engine.request.mockImplementation((c) =>
      Promise.resolve(
        c.cmd === 'rename_workspace'
          ? { reply: 'error', code: 'bad_request', message: 'Another workspace is called “Brand”.', hint: null }
          : list(['Ship', 'Brand']),
      ),
    );
    page();
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Rename…' }));
    const field = screen.getByRole('textbox', { name: 'New name' });
    expect(document.activeElement).toBe(field);
    await userEvent.clear(field);
    await userEvent.type(field, 'brand');
    await userEvent.click(screen.getByRole('button', { name: 'Save name' }));
    expect((await screen.findAllByRole('alert')).map((a) => a.textContent)).toContain(
      'Another workspace is called “Brand”.',
    );
  });

  it('records a shortcut for a workspace with Mosaïk’s own shortcuts held', async () => {
    engine.request.mockImplementation((c) =>
      Promise.resolve(c.cmd === 'workspaces' ? list(['Ship']) : { reply: 'done', message: 'ok' }),
    );
    page();
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Change shortcut…' }));
    expect(await screen.findByRole('button', { name: 'Press the new shortcut' })).toBeTruthy();
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'hold_hotkeys', hold: true });
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('button', { name: 'Press the new shortcut' })).toBeNull();
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'hold_hotkeys', hold: false });
  });
  it('shows a workspace’s apps in their zones, and moves, removes and adds them', async () => {
    let apps: WorkspaceApps = {
      workspace: 'w0',
      name: 'Ship',
      apps: [
        { id: 's1', label: 'Visual Studio Code', zone: 'z1', tab: false },
        { id: 's2', label: 'Windows Terminal', zone: 'z2', tab: false },
      ],
      zones: [
        { id: 'z1', name: 'Code', color: 'lapis' },
        { id: 'z2', name: 'Terminal', color: 'verdigris' },
      ],
      windows: [{ window: 0x204f2, label: 'Google Chrome', title: 'Release notes - Google Chrome' }],
    };
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'workspaces') return Promise.resolve(list(['Ship']));
      if (c.cmd === 'move_app') {
        apps = { ...apps, apps: apps.apps.map((a) => (a.id === c.app ? { ...a, zone: c.zone } : a)) };
      } else if (c.cmd === 'remove_app') {
        apps = { ...apps, apps: apps.apps.filter((a) => a.id !== c.app) };
      } else if (c.cmd === 'add_app') {
        apps = {
          ...apps,
          apps: [...apps.apps, { id: 's3', label: 'Google Chrome', zone: null, tab: false }],
          windows: [],
        };
      } else if (c.cmd !== 'apps') {
        return Promise.resolve({ reply: 'done', message: 'ok' });
      }
      return Promise.resolve({ reply: 'apps', ...apps });
    });
    page();
    const toggle = await screen.findByRole('button', { name: 'Apps of “Ship”' });
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    await userEvent.click(toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    const listed = await screen.findByRole('list', { name: 'Apps of “Ship”' });
    expect(
      within(listed)
        .getAllByRole('listitem')
        .map((li) => li.textContent),
    ).toEqual([expect.stringContaining('Visual Studio Code'), expect.stringContaining('Windows Terminal')]);
    const zone = screen.getByRole('combobox', { name: 'Zone of Windows Terminal' });
    expect((zone as HTMLSelectElement).value).toBe('z2');

    await userEvent.selectOptions(zone, 'Code');
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'move_app', workspace: 'w0', app: 's2', zone: 'z1' });
    expect(await screen.findByText('Windows Terminal goes in Code.')).toBeTruthy();

    await userEvent.click(screen.getByRole('button', { name: 'Remove Visual Studio Code' }));
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'remove_app', workspace: 'w0', app: 's1' });
    expect(await screen.findByText('Visual Studio Code removed.')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Remove Visual Studio Code' })).toBeNull();

    await userEvent.click(screen.getByRole('button', { name: 'Add a window to “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: /^Google Chrome/ }));
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'add_app', workspace: 'w0', window: 0x204f2, zone: null });
    expect(await screen.findByText('Google Chrome added.')).toBeTruthy();
    // Without a zone yet, it says so; with no other window open, it says how to add one.
    const chrome = screen.getByRole('combobox', { name: 'Zone of Google Chrome' });
    expect((chrome as HTMLSelectElement).value).toBe('');
    expect(screen.getByText('Open a window to add it here.')).toBeTruthy();
  });

  it('says why an app could not be changed', async () => {
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'workspaces') return Promise.resolve(list(['Ship']));
      if (c.cmd === 'remove_app') {
        return Promise.resolve({
          reply: 'error',
          code: 'bad_request',
          message: 'This app is not in the workspace any more.',
          hint: 'Look at the list again.',
        });
      }
      return Promise.resolve({
        reply: 'apps',
        workspace: 'w0',
        name: 'Ship',
        apps: [{ id: 's1', label: 'Notepad', zone: null, tab: false }],
        zones: [],
        windows: [],
      });
    });
    page();
    await userEvent.click(await screen.findByRole('button', { name: 'Apps of “Ship”' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Remove Notepad' }));
    expect(await screen.findByText('This app is not in the workspace any more. Look at the list again.')).toBeTruthy();
    // No zones: no list of zones to pick from.
    expect(screen.queryByRole('combobox')).toBeNull();
  });
});
