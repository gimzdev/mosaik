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
  setsReply,
  shown,
  workspace,
  workspacesReply,
} from './test-fixtures';
import { Announcer } from './ui';

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

/** The screen as the smart layout would leave it already. */
const arranged: ArrangePreview = { ...coding, windows: coding.windows.map((w) => ({ ...w, now: w.then ?? w.now })) };

/** The smart layout with the editor and the browser traded: each keeps its colour, each zone its
 * number. */
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

function answer(smart: ArrangePreview = coding, extra: (c: Command) => Reply | null = () => null) {
  const asked: Command[] = [];
  engine.request.mockImplementation((c) => {
    asked.push(c);
    const special = extra(c);
    if (special) return Promise.resolve(special);
    switch (c.cmd) {
      case 'preview_arrange':
        if (c.preset === 'full') return Promise.resolve(preview(full));
        if (c.preset === 'quarters') return Promise.resolve(preview(quarters));
        if (c.preset) return Promise.resolve(preview(halves));
        return Promise.resolve(preview(c.moves?.length ? traded : smart));
      case 'arrange':
        return Promise.resolve(done('Arranged 2 windows for coding.'));
      case 'undo_arrange':
        return Promise.resolve(done('Windows are back where they were.'));
      case 'workspaces':
        return Promise.resolve(workspacesReply([]));
      case 'sets':
        return Promise.resolve(setsReply([]));
      default:
        return Promise.resolve(done('Done.'));
    }
  });
  return asked;
}

function home() {
  render(
    <Announcer>
      <Home onEditWorkspace={vi.fn()} onEditZones={vi.fn()} />
    </Announcer>,
  );
}

const picture = () => screen.getByRole('group', { name: /main screen|Visual Studio Code|Nothing is open/ });
const tile = (name: string) => screen.getByRole('button', { name });
const arrange = () => screen.getByRole('button', { name: 'Arrange' });

describe('Home', () => {
  it('shows the smart layout for what is open, each window in its zone, before anything moves', async () => {
    const asked = answer();
    home();
    await waitFor(() => {
      expect(picture()).toHaveAttribute('aria-label', 'Visual Studio Code in zone 1, Google Chrome in zone 2.');
    });
    expect(screen.getByText('Smart layout for coding')).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Smart' })).toBeChecked();
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
    expect(asked.some((c) => c.cmd === 'arrange')).toBe(false);
  });

  it('names the windows a layout with fewer zones would minimise', async () => {
    answer();
    home();
    await userEvent.click(await screen.findByRole('radio', { name: 'Full screen' }));
    expect(await screen.findByText('Google Chrome will be minimized.')).toBeInTheDocument();
    expect(picture()).toHaveAttribute('aria-label', 'Visual Studio Code in zone 1, Google Chrome minimized.');
  });

  it('arranges with the smart layout, says what it did, and undoes it', async () => {
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
    expect(await screen.findByText('Open some apps first')).toBeInTheDocument();
    expect(arrange()).toBeDisabled();
    expect(screen.getByRole('radio', { name: 'Halves' })).toBeDisabled();
    expect(picture()).toHaveAttribute('aria-label', 'Nothing is open on your main screen.');
  });

  it('says when Mosaïk cannot read the screen', async () => {
    answer(coding, (c) => (c.cmd === 'preview_arrange' ? failed('Safe mode: Mosaïk does not read windows.') : null));
    home();
    expect(await screen.findByRole('alert')).toHaveTextContent('Safe mode: Mosaïk does not read windows.');
    expect(screen.getByRole('group', { name: 'Your main screen could not be read.' })).toBeInTheDocument();
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
    expect(screen.queryByRole('button', { name: 'Reset' })).not.toBeInTheDocument();
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

  it('puts the windows back with Reset, and a layout chosen starts afresh', async () => {
    const asked = answer();
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Visual Studio Code in zone 1' }));
    await userEvent.click(tile('Google Chrome in zone 2'));
    await userEvent.click(await screen.findByRole('button', { name: 'Reset' }));
    expect(screen.queryByRole('button', { name: 'Reset' })).not.toBeInTheDocument();
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
    expect(screen.queryByRole('button', { name: 'Reset' })).not.toBeInTheDocument();
  });

  it('shows a workspace picked as opening it would leave the screen, and opens it with Open', async () => {
    // The owner: "we shouldnt open the workspace when clicking on it it should show it in the
    // anim as preview and show an open button same as when you edit the workspace".
    const ship = workspace({ id: 'ship', name: 'Ship' });
    const asked = answer(coding, (c) => {
      if (c.cmd === 'workspaces') return workspacesReply([ship]);
      if (c.cmd === 'preview_workspace') return preview(opening);
      if (c.cmd === 'switch') return done('Opened “Ship”; starting 1 app.');
      return null;
    });
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Ship' }));
    expect(asked).toContainEqual({ cmd: 'preview_workspace', workspace: 'ship' });
    await waitFor(() => {
      expect(picture()).toHaveAttribute(
        'aria-label',
        'Visual Studio Code in zone 1, Windows Terminal starts in zone 2 with Claude as tabs, Google Chrome minimized.',
      );
    });
    expect(screen.getByText('As “Ship” opens')).toBeInTheDocument();
    expect(screen.getByText('Windows Terminal will start. Google Chrome will be minimized.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Ship' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.queryByRole('button', { name: 'Arrange' })).not.toBeInTheDocument();
    expect(asked.some((c) => c.cmd === 'switch')).toBe(false);
    await userEvent.click(screen.getByRole('button', { name: 'Open “Ship”' }));
    expect(asked).toContainEqual({ cmd: 'switch', workspace: 'ship' });
    expect(await shown('Opened “Ship”; starting 1 app.')).toBeInTheDocument();
    // Open, it is let go: Arrange is back.
    expect(screen.getByRole('button', { name: 'Arrange' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Ship' })).toHaveAttribute('aria-pressed', 'false');
  });

  it('goes back to Arrange when a layout is chosen, or the workspace shown is clicked again', async () => {
    const ship = workspace({ id: 'ship', name: 'Ship' });
    answer(coding, (c) => {
      if (c.cmd === 'workspaces') return workspacesReply([ship]);
      if (c.cmd === 'preview_workspace') return preview(opening);
      return null;
    });
    home();
    await userEvent.click(await screen.findByRole('button', { name: 'Ship' }));
    expect(await screen.findByRole('button', { name: 'Open “Ship”' })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Smart' })).not.toBeChecked();
    await userEvent.click(screen.getByRole('radio', { name: 'Halves' }));
    expect(screen.getByRole('button', { name: 'Arrange' })).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Halves' })).toBeChecked();
    await userEvent.click(screen.getByRole('button', { name: 'Ship' }));
    expect(await screen.findByRole('button', { name: 'Open “Ship”' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Ship' }));
    expect(screen.getByRole('button', { name: 'Arrange' })).toBeInTheDocument();
  });

  it('offers Smart, then the ten layouts by name, beside the workspaces', async () => {
    answer();
    home();
    const area = await screen.findByRole('region', { name: 'Your screen' });
    const group = within(area).getByRole('group', { name: 'Layout' });
    expect(
      within(group)
        .getAllByRole('radio')
        .map((r) => r.getAttribute('value')),
    ).toEqual([
      'smart',
      'halves',
      'thirds',
      'main-side',
      'side-main',
      'half-stack',
      'stack-half',
      'quarters',
      'rows',
      'three-rows',
      'full',
    ]);
    expect(within(group).getByRole('radio', { name: 'Main + side' })).toBeInTheDocument();
    expect(screen.getByRole('region', { name: 'Workspaces' })).toBeInTheDocument();
  });
});
