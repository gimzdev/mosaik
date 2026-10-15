import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { AppSet, Command, Reply, WorkspaceSummary } from './protocol';
import { done, failed, set, setsReply, workspace, workspacesReply } from './test-fixtures';
import { Announcer } from './ui';
import { type Shown, type Sorted, type Target, Workspaces } from './Workspaces';

const engine = vi.hoisted(() => ({
  request: vi.fn<(c: Command) => Promise<Reply>>(),
  appIcons: vi.fn((keys: string[]) =>
    Promise.resolve(Object.fromEntries(keys.map((k) => [k, k === 'app:Chrome' ? 'data:image/png;base64,AA==' : null]))),
  ),
  onWorkspacesChanged: vi.fn<(callback: () => void) => () => void>(() => () => undefined),
}));
vi.mock('./engine', () => engine);

/** The engine's "workspaces changed" event. */
let changed: () => void = () => undefined;

afterEach(cleanup);
beforeEach(() => {
  engine.request.mockReset();
  changed = () => undefined;
  engine.onWorkspacesChanged.mockImplementation((callback) => {
    changed = callback;
    return () => undefined;
  });
});

const ship = workspace({ id: 'ship', name: 'Ship', hotkey: 'Win+Alt+J' });
const brand = workspace({ id: 'brand', name: 'Brand', apps: ['Figma'], icons: [null] });
const coding = set({
  id: 'coding',
  name: 'Coding',
  apps: ['Visual Studio Code', 'Terminal', 'Google Chrome'],
});

/** Commands after which the engine says the workspaces changed (crates/mosaik-engine/src/thread.rs). */
const CHANGES: ReadonlySet<Command['cmd']> = new Set(['save', 'switch', 'open_set']);

function answer(
  list: WorkspaceSummary[] = [ship, brand],
  sets: AppSet[] = [coding],
  extra: (c: Command) => Reply | null = () => null,
  active: string | null = 'ship',
) {
  const asked: Command[] = [];
  engine.request.mockImplementation((c) => {
    asked.push(c);
    const reply = extra(c) ?? reply_(c);
    if (CHANGES.has(c.cmd) && reply.reply !== 'error') setTimeout(changed, 0);
    return Promise.resolve(reply);
  });
  function reply_(c: Command): Reply {
    switch (c.cmd) {
      case 'workspaces':
        return workspacesReply(list, active);
      case 'sets':
        return setsReply(sets);
      case 'save':
        return done(`Saved “${c.workspace ?? 'Ship'}”.`);
      default:
        return done('Done.');
    }
  }
  return asked;
}

function render_(options: { shown?: Target | null; launching?: Target | null; sorted?: Sorted | null } = {}) {
  const props = {
    onEdit: vi.fn(),
    onEditZones: vi.fn(),
    onShow: vi.fn<(s: Shown) => void>(),
    onOpen: vi.fn<(w: WorkspaceSummary) => void>(),
    onSay: vi.fn<(text: string, error?: boolean) => void>(),
    onSortedRead: vi.fn(),
  };
  render(
    <Announcer>
      <Workspaces
        shown={options.shown ?? null}
        launching={options.launching ?? null}
        sorted={options.sorted ?? null}
        {...props}
      />
    </Announcer>,
  );
  return props;
}

const list = () => screen.getByRole('list', { name: 'Workspaces' });

const wait = (ms: number) =>
  new Promise((resolve) => {
    setTimeout(resolve, ms);
  });

const count = (asked: Command[], cmd: Command['cmd']) => asked.filter((c) => c.cmd === cmd).length;

describe('the workspaces', () => {
  it('shows each with its apps, the one on screen marked in quiet words', async () => {
    answer();
    render_();
    const open = await screen.findByRole('button', { name: 'Ship (on screen)' });
    expect(within(open).getByText('On screen')).toBeInTheDocument();
    // The shortcut is on its page, not in the row.
    expect(within(open).queryByText('Win + Alt + J')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Brand' })).toBeInTheDocument();
    // Apps show by their icons, read once for every app on the page.
    await waitFor(() => {
      expect(open.querySelector('img[src="data:image/png;base64,AA=="]')).not.toBeNull();
    });
    expect(engine.appIcons).toHaveBeenCalled();
  });

  it('shows one in a click, without opening it, and opens it in a double-click', async () => {
    // The owner: "we shouldnt open the workspace when clicking on it it should show it in the
    // anim as preview and show an open button".
    const asked = answer();
    const { onShow, onOpen } = render_();
    await userEvent.click(await screen.findByRole('button', { name: 'Brand' }));
    expect(onShow).toHaveBeenCalledWith({ kind: 'workspace', workspace: brand });
    expect(onOpen).not.toHaveBeenCalled();
    expect(asked.some((c) => c.cmd === 'switch')).toBe(false);
    await userEvent.dblClick(screen.getByRole('button', { name: 'Brand' }));
    expect(onOpen).toHaveBeenCalledWith(brand);
  });

  it('marks the one Home shows, and the one being opened', async () => {
    answer();
    render_({ shown: { kind: 'workspace', id: 'brand' }, launching: { kind: 'workspace', id: 'brand' } });
    const row = await screen.findByRole('button', { name: 'Brand' });
    expect(row).toHaveAttribute('aria-pressed', 'true');
    expect(row).toHaveAttribute('aria-busy', 'true');
    expect(screen.getByRole('button', { name: 'Ship (on screen)' })).toHaveAttribute('aria-pressed', 'false');
  });

  it('opens one from its menu', async () => {
    answer();
    const { onOpen } = render_();
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Brand”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Open' }));
    expect(onOpen).toHaveBeenCalledWith(brand);
  });

  it('suggests workspaces from the apps installed, and shows one in a click without starting it', async () => {
    const asked = answer();
    const { onShow } = render_();
    const suggested = await screen.findByRole('button', {
      name: 'Coding (suggested): Visual Studio Code, Terminal, Google Chrome',
    });
    expect(within(suggested).getByText('3 apps that go together')).toBeInTheDocument();
    expect(screen.getByRole('list', { name: 'Suggested workspaces' })).toContainElement(suggested);
    await userEvent.click(suggested);
    expect(onShow).toHaveBeenCalledWith({ kind: 'set', set: coding });
    expect(asked.some((c) => c.cmd === 'open_set')).toBe(false);
  });

  it('does not suggest a set that is a workspace already', async () => {
    answer([ship], [{ ...coding, workspace: 'ship' }]);
    render_();
    await screen.findByRole('button', { name: 'Ship (on screen)' });
    expect(screen.queryByRole('button', { name: /\(suggested\)/ })).not.toBeInTheDocument();
  });

  it('keeps what is on screen as a new workspace, under a name that is free', async () => {
    const asked = answer();
    const { onSay } = render_();
    await userEvent.click(await screen.findByRole('button', { name: 'Save as workspace' }));
    const name = screen.getByRole('textbox', { name: 'Name the new workspace' });
    expect(name).toHaveValue('Workspace 1');
    await userEvent.clear(name);
    await userEvent.type(name, 'Ship');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    // A name in use would replace that workspace: refused, nothing sent. Ship is on screen: its
    // menu can take what is on screen.
    expect(screen.getByRole('alert')).toHaveTextContent(
      '“Ship” already exists. Pick another name, or open its ⋯ menu and choose Replace with what’s on screen.',
    );
    expect(asked.some((c) => c.cmd === 'save')).toBe(false);
    await userEvent.clear(name);
    await userEvent.type(name, 'Brand');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    // Brand is not on screen: its menu has nothing to replace it with.
    expect(screen.getByRole('alert')).toHaveTextContent(/^“Brand” already exists\. Pick another name\.$/);
    await userEvent.clear(name);
    await userEvent.type(name, 'Review{Enter}');
    expect(asked).toContainEqual({ cmd: 'save', workspace: 'Review' });
    await waitFor(() => {
      expect(onSay).toHaveBeenCalledWith('Saved “Review”.');
    });
    expect(screen.queryByRole('textbox', { name: 'Name the new workspace' })).not.toBeInTheDocument();
  });

  it('leaves saving with Escape', async () => {
    answer();
    render_();
    await userEvent.click(await screen.findByRole('button', { name: 'Save as workspace' }));
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('textbox', { name: 'Name the new workspace' })).not.toBeInTheDocument();
  });

  it('has the rest in each one’s menu: replacing the one on screen, its page, its zones', async () => {
    const asked = answer();
    const { onEdit, onEditZones, onSay } = render_();
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Replace with what’s on screen' }));
    expect(asked).toContainEqual({ cmd: 'save', workspace: null });
    await waitFor(() => {
      expect(onSay).toHaveBeenCalledWith('Saved “Ship”.');
    });
    await userEvent.click(screen.getByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Edit' }));
    expect(onEdit).toHaveBeenCalledWith('ship');
    await userEvent.click(screen.getByRole('button', { name: 'More for “Brand”' }));
    // Only the workspace on screen can take what is on screen.
    expect(screen.queryByRole('button', { name: 'Replace with what’s on screen' })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Edit zones' }));
    expect(onEditZones).toHaveBeenCalledWith('brand');
  });

  it('says why the one on screen could not take what is on screen', async () => {
    answer([ship, brand], [], (c) => (c.cmd === 'save' ? failed('There are no windows on screen to save.') : null));
    const { onSay } = render_();
    await userEvent.click(await screen.findByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Replace with what’s on screen' }));
    await waitFor(() => {
      expect(onSay).toHaveBeenCalledWith('There are no windows on screen to save.', true);
    });
  });

  it('reads them once after its own change, and not for a burst of focus events', async () => {
    const asked = answer();
    render_();
    await screen.findByRole('button', { name: 'Ship (on screen)' });
    await wait(300);
    const before = { workspaces: count(asked, 'workspaces'), sets: count(asked, 'sets') };
    for (let i = 0; i < 3; i += 1) {
      window.dispatchEvent(new Event('focus'));
      await wait(40);
    }
    await wait(400);
    // The workspaces change only by Mosaïk's say-so; the apps installed, rarely.
    expect(count(asked, 'workspaces') - before.workspaces).toBe(0);
    expect(count(asked, 'sets') - before.sets).toBe(0);
    await userEvent.click(screen.getByRole('button', { name: 'More for “Ship”' }));
    await userEvent.click(screen.getByRole('button', { name: 'Replace with what’s on screen' }));
    await wait(400);
    expect(count(asked, 'workspaces') - before.workspaces).toBe(1);
    expect(count(asked, 'sets') - before.sets).toBe(1);
  });

  it('reads them again when they change elsewhere', async () => {
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
      await screen.findByText('Arrange your windows, then save them as a workspace to bring them back later.'),
    ).toBeInTheDocument();
    expect(list()).toBeInTheDocument();
  });

  it('says the first run made workspaces of the windows open, until put away', async () => {
    answer([ship, brand]);
    const { onSortedRead } = render_({ sorted: { before: new Set(['brand']), error: null } });
    const note = await screen.findByText('Made from the windows you had open. Delete any you don’t need.');
    await userEvent.click(screen.getByRole('button', { name: 'Got it' }));
    expect(onSortedRead).toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Got it' })).toHaveAccessibleDescription(note.textContent);
  });

  it('says nothing of the first run when it made no workspace, and why when it could not', async () => {
    answer([ship]);
    render_({ sorted: { before: new Set(['ship']), error: null } });
    await screen.findByRole('button', { name: 'Ship (on screen)' });
    expect(screen.queryByRole('button', { name: 'Got it' })).not.toBeInTheDocument();
    cleanup();
    answer([ship]);
    render_({ sorted: { before: null, error: 'Workspaces cannot be saved: the disk is full.' } });
    expect(await screen.findByRole('alert')).toHaveTextContent('Workspaces cannot be saved: the disk is full.');
  });
});
