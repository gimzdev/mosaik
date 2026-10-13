import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Command, Reply } from './protocol';
import { appsOf, done, failed, shown, workspace, workspacesReply } from './test-fixtures';
import { Announcer } from './ui';
import { WorkspacePage } from './WorkspacePage';

const engine = vi.hoisted(() => ({
  request: vi.fn<(c: Command) => Promise<Reply>>(),
  appIcons: vi.fn((keys: string[]) => Promise.resolve(Object.fromEntries(keys.map((k) => [k, null])))),
  onWorkspacesChanged: vi.fn(() => () => undefined),
}));
vi.mock('./engine', () => engine);

afterEach(cleanup);
beforeEach(() => {
  engine.request.mockReset();
});

function answer(extra: (c: Command) => Reply | null = () => null, exists = true) {
  const asked: Command[] = [];
  let name = 'Ship';
  let hotkey: string | null = null;
  engine.request.mockImplementation((c) => {
    asked.push(c);
    const special = extra(c);
    if (special) return Promise.resolve(special);
    switch (c.cmd) {
      case 'workspaces':
        return Promise.resolve(workspacesReply(exists ? [workspace({ id: 'ship', name, hotkey })] : [], 'ship'));
      case 'apps':
        return Promise.resolve(appsOf({ name }));
      case 'rename_workspace':
        name = c.name;
        return Promise.resolve(done(`Renamed to “${c.name}”.`));
      case 'set_workspace_hotkey':
        hotkey = c.hotkey;
        return Promise.resolve(done('Saved.'));
      case 'check_hotkey':
        return Promise.resolve({ reply: 'hotkey', hotkey: c.hotkey, problem: null, warnings: [] });
      case 'move_app':
        return Promise.resolve(
          appsOf({
            apps: [
              { id: 's1', label: 'Visual Studio Code', icon: null, zone: 'z2', tab: false },
              { id: 's2', label: 'Google Chrome', icon: null, zone: 'z2', tab: false },
            ],
          }),
        );
      case 'remove_app':
        return Promise.resolve(
          appsOf({ apps: [{ id: 's2', label: 'Google Chrome', icon: null, zone: 'z2', tab: false }] }),
        );
      case 'add_app':
        return Promise.resolve(
          appsOf({
            apps: [
              ...(appsOf() as Extract<Reply, { reply: 'apps' }>).apps,
              { id: 's3', label: 'Slack', icon: null, zone: null, tab: false },
            ],
            windows: [],
          }),
        );
      default:
        return Promise.resolve(done('Done.'));
    }
  });
  return asked;
}

function page(onBack = vi.fn(), onEditZones = vi.fn()) {
  render(
    <Announcer>
      <WorkspacePage workspace="ship" onBack={onBack} onEditZones={onEditZones} />
    </Announcer>,
  );
  return { onBack, onEditZones };
}

describe('a workspace’s page', () => {
  it('shows its apps in their zones, and moves, removes and adds them', async () => {
    const asked = answer();
    page();
    const apps = await screen.findByRole('list', { name: 'Apps of “Ship”' });
    expect(within(apps).getByText('Visual Studio Code')).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Zone of Visual Studio Code' }), 'Browser');
    expect(asked).toContainEqual({ cmd: 'move_app', workspace: 'ship', app: 's1', zone: 'z2' });
    expect(await shown('Visual Studio Code goes in Browser.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Remove Visual Studio Code' }));
    expect(asked).toContainEqual({ cmd: 'remove_app', workspace: 'ship', app: 's1' });
    expect(await shown('Visual Studio Code removed.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Add a window to “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: /^Slack/ }));
    expect(asked).toContainEqual({ cmd: 'add_app', workspace: 'ship', window: 0x2_04f2, zone: null });
  });

  it('says why an app could not be changed', async () => {
    answer((c) => (c.cmd === 'remove_app' ? failed('The workspace could not be saved.') : null));
    page();
    await userEvent.click(await screen.findByRole('button', { name: 'Remove Visual Studio Code' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('The workspace could not be saved.');
  });

  it('renames it, and says why a name was refused', async () => {
    const asked = answer((c) =>
      c.cmd === 'rename_workspace' && c.name === 'Brand' ? failed('“Brand” is taken.') : null,
    );
    page();
    await userEvent.click(await screen.findByRole('button', { name: 'Rename “Ship”' }));
    const name = screen.getByRole('textbox', { name: 'Name' });
    await userEvent.clear(name);
    await userEvent.type(name, 'Brand{Enter}');
    expect(await screen.findByRole('alert')).toHaveTextContent('“Brand” is taken.');
    await userEvent.clear(name);
    await userEvent.type(name, 'Ship 2{Enter}');
    expect(asked).toContainEqual({ cmd: 'rename_workspace', workspace: 'ship', name: 'Ship 2' });
    expect(await screen.findByRole('heading', { name: 'Ship 2', level: 1 })).toBeInTheDocument();
  });

  it('records a shortcut of its own, with Mosaïk’s shortcuts held meanwhile', async () => {
    const asked = answer();
    page();
    await userEvent.click(await screen.findByRole('button', { name: 'Add a shortcut: Open “Ship”' }));
    expect(asked).toContainEqual({ cmd: 'hold_hotkeys', hold: true });
    await userEvent.keyboard('{Meta>}{Alt>}j{/Alt}{/Meta}');
    await userEvent.click(await screen.findByRole('button', { name: 'Save shortcut' }));
    expect(asked).toContainEqual({ cmd: 'set_workspace_hotkey', workspace: 'ship', hotkey: 'Win+Alt+J' });
    expect(asked).toContainEqual({ cmd: 'hold_hotkeys', hold: false });
  });

  it('opens its zones', async () => {
    answer();
    const { onEditZones } = page();
    await userEvent.click(await screen.findByRole('button', { name: 'Edit zones' }));
    expect(onEditZones).toHaveBeenCalledWith('ship');
  });

  it('opens it', async () => {
    const asked = answer();
    page();
    await userEvent.click(await screen.findByRole('button', { name: 'Open' }));
    expect(asked).toContainEqual({ cmd: 'switch', workspace: 'ship' });
  });

  it('asks before deleting, then goes back', async () => {
    const asked = answer();
    const { onBack } = page();
    await userEvent.click(await screen.findByRole('button', { name: 'Delete workspace' }));
    const dialog = screen.getByRole('dialog', { name: 'Delete “Ship”?' });
    expect(asked.some((c) => c.cmd === 'delete_workspace')).toBe(false);
    await userEvent.click(within(dialog).getByRole('button', { name: 'Delete workspace' }));
    expect(asked).toContainEqual({ cmd: 'delete_workspace', workspace: 'ship' });
    await vi.waitFor(() => {
      expect(onBack).toHaveBeenCalled();
    });
  });

  it('says when the workspace is gone, with a way back', async () => {
    answer(() => null, false);
    const { onBack } = page();
    expect(await screen.findByRole('heading', { name: 'This workspace is gone' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Back to Home' }));
    expect(onBack).toHaveBeenCalled();
  });
});
