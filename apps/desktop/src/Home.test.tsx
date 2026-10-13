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
  preview,
  said,
  setsReply,
  shown,
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
    { name: 'Left', color: 'lapis', rect: { left: 0, top: 0, right: 960, bottom: 1032 } },
    { name: 'Right', color: 'verdigris', rect: { left: 960, top: 0, right: 1920, bottom: 1032 } },
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
  zones: [{ name: 'Screen', color: 'lapis', rect: WORK }],
  windows: coding.windows.map((w, i) => (i === 0 ? { ...w, then: WORK } : { ...w, then: null, minimized: true })),
};

/** The screen as the smart layout would leave it already. */
const arranged: ArrangePreview = { ...coding, windows: coding.windows.map((w) => ({ ...w, now: w.then ?? w.now })) };

function answer(smart: ArrangePreview = coding, extra: (c: Command) => Reply | null = () => null) {
  const asked: Command[] = [];
  engine.request.mockImplementation((c) => {
    asked.push(c);
    const special = extra(c);
    if (special) return Promise.resolve(special);
    switch (c.cmd) {
      case 'preview_arrange':
        if (c.preset === 'full') return Promise.resolve(preview(full));
        if (c.preset) return Promise.resolve(preview(halves));
        return Promise.resolve(preview(smart));
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

const picture = () => screen.getByRole('img', { name: /main screen|Visual Studio Code|Nothing is open/ });
const arrange = () => screen.getByRole('button', { name: 'Arrange' });

describe('Home', () => {
  it('shows the smart layout for what is open, each window in its zone, before anything moves', async () => {
    const asked = answer();
    home();
    await waitFor(() => {
      expect(picture()).toHaveAttribute('aria-label', 'Visual Studio Code in Code, Google Chrome in Browser.');
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
      expect(picture()).toHaveAttribute('aria-label', 'Visual Studio Code in Left, Google Chrome in Right.');
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
    expect(picture()).toHaveAttribute('aria-label', 'Visual Studio Code in Screen, Google Chrome minimized.');
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
    expect(screen.getByRole('img', { name: 'Your main screen could not be read.' })).toBeInTheDocument();
  });

  it('offers Smart, then the ten layouts by name, beside the workspaces', async () => {
    answer();
    home();
    const area = await screen.findByRole('region', { name: 'Your screen' });
    const group = within(area).getByRole('group', { name: 'Layout' });
    expect(within(group).getAllByRole('radio').map((r) => r.getAttribute('value'))).toEqual([
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
