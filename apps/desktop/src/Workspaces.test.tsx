import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { AppSet, Command, Reply, WorkspaceSummary } from './protocol';
import { done, failed, set, setsReply, shown, workspace, workspacesReply } from './test-fixtures';
import { Announcer } from './ui';
import { Workspaces } from './Workspaces';

const engine = vi.hoisted(() => ({
  request: vi.fn<(c: Command) => Promise<Reply>>(),
  appIcons: vi.fn((keys: string[]) =>
    Promise.resolve(Object.fromEntries(keys.map((k) => [k, k === 'app:Chrome' ? 'data:image/png;base64,AA==' : null]))),
  ),
  onWorkspacesChanged: vi.fn<(callback: () => void) => () => void>(() => () => undefined),
}));
vi.mock('./engine', () => engine);

afterEach(cleanup);
beforeEach(() => {
  engine.request.mockReset();
});

const ship = workspace({ id: 'ship', name: 'Ship', hotkey: 'Win+Alt+J' });
const brand = workspace({ id: 'brand', name: 'Brand', apps: ['Figma'], icons: [null] });
const coding = set({
  id: 'coding',
  name: 'Coding',
  apps: ['Visual Studio Code', 'Terminal', 'Google Chrome'],
});

function answer(
  list: WorkspaceSummary[] = [ship, brand],
  sets: AppSet[] = [coding],
  extra: (c: Command) => Reply | null = () => null,
  active: string | null = 'ship',
) {
  const asked: Command[] = [];
  engine.request.mockImplementation((c) => {
    asked.push(c);
    const special = extra(c);
    if (special) return Promise.resolve(special);
    switch (c.cmd) {
      case 'workspaces':
        return Promise.resolve(workspacesReply(list, active));
      case 'sets':
        return Promise.resolve(setsReply(sets));
      case 'switch':
        return Promise.resolve(done(`Opened “${c.workspace}”.`));
      case 'open_set':
        return Promise.resolve(done('Opened “Coding”: 2 apps started.'));
      case 'save':
        return Promise.resolve(done(`Saved “${c.workspace ?? 'Ship'}”.`));
      default:
        return Promise.resolve(done('Done.'));
    }
  });
  return asked;
}

function render_(onEdit = vi.fn(), onEditZones = vi.fn(), onOpened = vi.fn(), picked: string | null = null) {
  const onPick = vi.fn<(w: WorkspaceSummary) => void>();
  render(
    <Announcer>
      <Workspaces picked={picked} onPick={onPick} onEdit={onEdit} onEditZones={onEditZones} onOpened={onOpened} />
    </Announcer>,
  );
  return { onEdit, onEditZones, onOpened, onPick };
}

const list = () => screen.getByRole('list', { name: 'Workspaces' });

describe('the workspaces', () => {
  it('shows each with its apps and shortcut, the one open marked', async () => {
    answer();
    render_();
    const open = await screen.findByRole('button', { name: 'Ship (open now)' });
    expect(within(open).getByText('Open now')).toBeInTheDocument();
    expect(within(open).getByText('Win + Alt + J')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Brand' })).toBeInTheDocument();
    // Apps show by their icons, read once for every app on the page.
    await waitFor(() => {
      expect(open.querySelector('img[src="data:image/png;base64,AA=="]')).not.toBeNull();
    });
    expect(engine.appIcons).toHaveBeenCalled();
  });

  it('picks one in a click, for Home to show, without opening it', async () => {
    // The owner: "we shouldnt open the workspace when clicking on it it should show it in the
    // anim as preview and show an open button".
    const asked = answer();
    const { onPick } = render_();
    await userEvent.click(await screen.findByRole('button', { name: 'Brand' }));
    expect(onPick).toHaveBeenCalledWith(brand);
    expect(asked.some((c) => c.cmd === 'switch')).toBe(false);
  });

  it('marks the one Home shows', async () => {
    answer();
    render_(vi.fn(), vi.fn(), vi.fn(), 'brand');
    expect(await screen.findByRole('button', { name: 'Brand' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: 'Ship (open now)' })).toHaveAttribute('aria-pressed', 'false');
  });

  it('opens one from its menu, and says so where it was opened', async () => {
    const asked = answer();
    const { onOpened } = render_();
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Brand”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Open' }));
    expect(asked).toContainEqual({ cmd: 'switch', workspace: 'brand' });
    expect(await shown('Opened “brand”.')).toBeInTheDocument();
    expect(onOpened).toHaveBeenCalled();
  });

  it('says why one could not be opened', async () => {
    answer([ship, brand], [], (c) => (c.cmd === 'switch' ? failed('Its monitor is gone.', 'Edit its zones.') : null));
    render_();
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Brand”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Open' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Its monitor is gone. Edit its zones.');
  });

  it('suggests workspaces from the apps installed, and opens one in a click', async () => {
    const asked = answer();
    render_();
    const suggested = await screen.findByRole('button', {
      name: 'Open the suggested workspace “Coding”: Visual Studio Code, Terminal, Google Chrome',
    });
    expect(within(suggested).getByText('3 apps that go together')).toBeInTheDocument();
    expect(screen.getByRole('list', { name: 'Suggested workspaces' })).toContainElement(suggested);
    await userEvent.click(suggested);
    expect(asked).toContainEqual({ cmd: 'open_set', set: 'coding' });
    expect(await shown('Opened “Coding”: 2 apps started.')).toBeInTheDocument();
  });

  it('does not suggest a set that is a workspace already', async () => {
    answer([ship], [{ ...coding, workspace: 'ship' }]);
    render_();
    await screen.findByRole('button', { name: 'Ship (open now)' });
    expect(screen.queryByRole('button', { name: /suggested workspace/ })).not.toBeInTheDocument();
  });

  it('keeps what is on screen as a new workspace, under a name that is free', async () => {
    const asked = answer();
    render_();
    await userEvent.click(await screen.findByRole('button', { name: 'Save layout' }));
    const name = screen.getByRole('textbox', { name: 'Name the new workspace' });
    expect(name).toHaveValue('Workspace 1');
    await userEvent.clear(name);
    await userEvent.type(name, 'Ship');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    // A name in use would replace that workspace: refused, nothing sent.
    expect(screen.getByRole('alert')).toHaveTextContent('“Ship” already exists.');
    expect(asked.some((c) => c.cmd === 'save')).toBe(false);
    await userEvent.clear(name);
    await userEvent.type(name, 'Review{Enter}');
    expect(asked).toContainEqual({ cmd: 'save', workspace: 'Review' });
    expect(await shown('Saved “Review”.')).toBeInTheDocument();
    expect(screen.queryByRole('textbox', { name: 'Name the new workspace' })).not.toBeInTheDocument();
  });

  it('leaves saving with Escape', async () => {
    answer();
    render_();
    await userEvent.click(await screen.findByRole('button', { name: 'Save layout' }));
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('textbox', { name: 'Name the new workspace' })).not.toBeInTheDocument();
  });

  it('has the rest in each one’s menu: saving the open one again, its page, its zones', async () => {
    const asked = answer();
    const { onEdit, onEditZones } = render_();
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Save current layout here' }));
    expect(asked).toContainEqual({ cmd: 'save', workspace: null });
    await userEvent.click(screen.getByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Edit' }));
    expect(onEdit).toHaveBeenCalledWith('ship');
    await userEvent.click(screen.getByRole('button', { name: 'More for “Brand”' }));
    // Only the open workspace can take what is on screen.
    expect(screen.queryByRole('button', { name: 'Save current layout here' })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Edit zones' }));
    expect(onEditZones).toHaveBeenCalledWith('brand');
  });

  it('reads them again when they change elsewhere', async () => {
    let changed: () => void = () => undefined;
    engine.onWorkspacesChanged.mockImplementation((callback) => {
      changed = callback;
      return () => undefined;
    });
    let names = [ship];
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'workspaces') return Promise.resolve(workspacesReply(names));
      if (c.cmd === 'sets') return Promise.resolve(setsReply([]));
      return Promise.resolve(done('Done.'));
    });
    render_();
    await screen.findByRole('button', { name: 'Ship' });
    names = [ship, brand];
    changed();
    expect(await screen.findByRole('button', { name: 'Brand' })).toBeInTheDocument();
  });

  it('says what workspaces are when there is none and nothing to suggest', async () => {
    answer([], []);
    render_();
    expect(
      await screen.findByText('Arrange your windows, then save the layout: it opens again in one click.'),
    ).toBeInTheDocument();
    expect(list()).toBeInTheDocument();
  });
});
