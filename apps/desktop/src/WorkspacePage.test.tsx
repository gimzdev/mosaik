import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Command, Reply } from './protocol';
import {
  appsOf,
  done,
  failed,
  opening,
  openingZones,
  preview,
  shown,
  workspace,
  workspacesReply,
  zonesReply,
} from './test-fixtures';
import { Announcer } from './ui';
import { WorkspacePage } from './WorkspacePage';

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

const count = (asked: Command[], cmd: Command['cmd']) => asked.filter((c) => c.cmd === cmd).length;

/** Commands after which the engine says the workspaces changed (crates/mosaik-engine/src/thread.rs). */
const CHANGES: ReadonlySet<Command['cmd']> = new Set([
  'switch',
  'rename_workspace',
  'delete_workspace',
  'set_workspace_hotkey',
  'add_app',
  'move_app',
  'remove_app',
  'set_zones',
]);

function answer(extra: (c: Command) => Reply | null = () => null, exists = true) {
  const asked: Command[] = [];
  let name = 'Ship';
  let hotkey: string | null = null;
  engine.request.mockImplementation((c) => {
    asked.push(c);
    const reply = extra(c) ?? usual(c);
    if (CHANGES.has(c.cmd) && reply.reply !== 'error') setTimeout(changed, 0);
    return Promise.resolve(reply);
  });
  function usual(c: Command): Reply {
    switch (c.cmd) {
      case 'workspaces':
        return workspacesReply(exists ? [workspace({ id: 'ship', name, hotkey })] : [], 'ship');
      case 'apps':
        return appsOf({ name });
      case 'preview_workspace':
        return preview(opening);
      case 'rename_workspace':
        name = c.name;
        return done(`Renamed to “${c.name}”.`);
      case 'set_workspace_hotkey':
        hotkey = c.hotkey;
        return done('Saved.');
      case 'check_hotkey':
        return { reply: 'hotkey', hotkey: c.hotkey, problem: null, warnings: [] };
      case 'zones':
      case 'set_zones':
        return zonesReply(openingZones);
      case 'move_app':
        return appsOf({
          apps: [
            { id: 's1', label: 'Visual Studio Code', icon: null, zone: 'z2', tab: false },
            { id: 's2', label: 'Google Chrome', icon: null, zone: 'z2', tab: false },
          ],
        });
      case 'remove_app':
        return appsOf({ apps: [{ id: 's2', label: 'Google Chrome', icon: null, zone: 'z2', tab: false }] });
      case 'add_app':
        return appsOf({
          apps: [
            ...(appsOf() as Extract<Reply, { reply: 'apps' }>).apps,
            { id: 's3', label: 'Slack', icon: null, zone: null, tab: false },
          ],
          windows: [],
        });
      default:
        return done('Done.');
    }
  }
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
  it('shows how it opens: its zones, with its apps in them', async () => {
    // The owner: "show visually the zones same as homepage for that workspace specifically with
    // arrangements aswell".
    const asked = answer();
    page();
    expect(
      await screen.findByRole('group', {
        name: 'Visual Studio Code in zone 1, Windows Terminal starts in zone 2 with Claude as tabs, Google Chrome minimized.',
      }),
    ).toBeInTheDocument();
    expect(asked).toContainEqual({ cmd: 'preview_workspace', workspace: 'ship' });
    // It is the one on screen, in quiet words.
    expect(screen.getByText('On screen')).toBeInTheDocument();
    expect(screen.getByText('2 apps · 2 zones')).toBeInTheDocument();
  });

  it('lists its apps zone by zone, and moves, removes and adds them', async () => {
    // The owner, of a row of zone dots for each app: "the options look like absolute garbage".
    const asked = answer();
    page();
    const apps = await screen.findByRole('group', { name: 'Apps of “Ship”' });
    const zone = (n: string) => within(apps).getByRole('group', { name: `Zone ${n}` });
    expect(within(zone('1')).getByText('Visual Studio Code')).toBeInTheDocument();
    expect(within(zone('2')).getByText('Google Chrome')).toBeInTheDocument();
    await userEvent.click(within(zone('1')).getByRole('button', { name: 'Move Visual Studio Code to another zone' }));
    // Only the other zones are offered.
    expect(screen.queryByRole('button', { name: 'Zone 1' })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Zone 2' }));
    expect(asked).toContainEqual({ cmd: 'move_app', workspace: 'ship', app: 's1', zone: 'z2' });
    expect(await shown('Visual Studio Code goes in zone 2.')).toBeInTheDocument();
    // A zone left empty says so, with its number.
    expect(within(zone('1')).getByText('Empty. Drag an app here on the picture, or use Move.')).toBeInTheDocument();
    expect(within(zone('2')).getByText('Visual Studio Code')).toBeInTheDocument();
    // The picture follows.
    await waitFor(() => {
      expect(asked.filter((c) => c.cmd === 'preview_workspace').length).toBeGreaterThan(1);
    });
    await userEvent.click(screen.getByRole('button', { name: 'Remove Visual Studio Code' }));
    expect(asked).toContainEqual({ cmd: 'remove_app', workspace: 'ship', app: 's1' });
    expect(await shown('Visual Studio Code removed.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Add a window to “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: /^Slack/ }));
    expect(asked).toContainEqual({ cmd: 'add_app', workspace: 'ship', window: 0x2_04f2, zone: null });
  });

  it('moves its apps and resizes its zones on its picture, saved at once', async () => {
    const asked = answer();
    page();
    await userEvent.click(await screen.findByRole('button', { name: 'Visual Studio Code in zone 1' }));
    await userEvent.click(screen.getByRole('button', { name: 'Zone 2' }));
    expect(asked).toContainEqual({ cmd: 'move_app', workspace: 'ship', app: 's1', zone: 'z2' });
    expect(await shown('Visual Studio Code goes in zone 2.')).toBeInTheDocument();
    // The list follows, from the answer.
    const apps = screen.getByRole('group', { name: 'Apps of “Ship”' });
    expect(within(within(apps).getByRole('group', { name: 'Zone 2' })).getByText('Visual Studio Code')).toBeVisible();
    const line = await screen.findByRole('separator', { name: 'Line between zone 1 and zone 2' });
    line.focus();
    await userEvent.keyboard('{ArrowLeft}');
    expect(await shown('Zones resized.')).toBeInTheDocument();
    const [z1, z2, other] = openingZones;
    expect(asked).toContainEqual({
      cmd: 'set_zones',
      workspace: 'ship',
      revision: '7',
      zones: [
        { ...z1, area: { left: 0, top: 0, right: 28 / 48, bottom: 1 } },
        { ...z2, area: { left: 28 / 48, top: 0, right: 1, bottom: 1 } },
        other,
      ],
    });
  });

  it('reads again once for a burst of focus events, and uses what a change of its apps answers', async () => {
    const asked = answer();
    page();
    await screen.findByRole('group', { name: 'Apps of “Ship”' });
    await wait(300);
    const before = { apps: count(asked, 'apps'), preview: count(asked, 'preview_workspace') };
    for (let i = 0; i < 3; i += 1) {
      window.dispatchEvent(new Event('focus'));
      await wait(40);
    }
    await wait(400);
    expect(count(asked, 'apps') - before.apps).toBe(1);
    expect(count(asked, 'preview_workspace') - before.preview).toBe(1);
    const then = {
      apps: count(asked, 'apps'),
      preview: count(asked, 'preview_workspace'),
      workspaces: count(asked, 'workspaces'),
    };
    await userEvent.click(screen.getByRole('button', { name: 'Move Visual Studio Code to another zone' }));
    await userEvent.click(screen.getByRole('button', { name: 'Zone 2' }));
    expect(await shown('Visual Studio Code goes in zone 2.')).toBeInTheDocument();
    await wait(400);
    // The apps came with the answer; the picture and the summary are read once more.
    expect(count(asked, 'apps') - then.apps).toBe(0);
    expect(count(asked, 'preview_workspace') - then.preview).toBe(1);
    expect(count(asked, 'workspaces') - then.workspaces).toBe(1);
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
    expect(await screen.findByText('Opens it from any app.')).toBeInTheDocument();
    await userEvent.click(await screen.findByRole('button', { name: 'Add a shortcut: Open “Ship”' }));
    expect(asked).toContainEqual({ cmd: 'hold_hotkeys', hold: true });
    await userEvent.keyboard('{Meta>}{Alt>}j{/Alt}{/Meta}');
    await userEvent.click(await screen.findByRole('button', { name: 'Save shortcut' }));
    expect(asked).toContainEqual({ cmd: 'set_workspace_hotkey', workspace: 'ship', hotkey: 'Win+Alt+J' });
    expect(asked).toContainEqual({ cmd: 'hold_hotkeys', hold: false });
    // Mosaïk says the workspace changed: the page shows the shortcut saved.
    expect(await screen.findByRole('button', { name: 'Change: Open “Ship”' })).toBeInTheDocument();
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
    expect(screen.getByText('It was deleted.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Back to Home' }));
    expect(onBack).toHaveBeenCalled();
  });
});
