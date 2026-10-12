import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { Command, Reply } from './protocol';
import { ZoneEditor } from './ZoneEditor';

const engine = vi.hoisted(() => ({ request: vi.fn<(c: Command) => Promise<Reply>>() }));
vi.mock('./engine', () => engine);

afterEach(cleanup);

const zones: Reply = {
  reply: 'zones',
  workspace: 'ship',
  name: 'Ship the release',
  monitors: [
    {
      id: 'm1',
      name: 'Screen',
      primary: true,
      bounds: { left: 0, top: 0, right: 1920, bottom: 1080 },
      work_area: { left: 0, top: 0, right: 1920, bottom: 1032 },
      dpi: 96,
      scale_percent: 100,
    },
  ],
  zones: [
    { id: 'z1', name: 'Code', color: 'lapis', monitor: 'm1', area: { left: 0, top: 0, right: 0.5, bottom: 1 } },
    { id: 'z2', name: 'Browser', color: 'verdigris', monitor: 'm1', area: { left: 0.5, top: 0, right: 1, bottom: 1 } },
  ],
};

describe('the zone editor', () => {
  it('resizes from the keyboard and saves', async () => {
    let saved: Command | null = null;
    engine.request.mockImplementation((c) => {
      if (c.cmd === 'set_zones') {
        saved = c;
        return Promise.resolve({ reply: 'done', message: 'Zones of “Ship the release” saved.' });
      }
      return Promise.resolve(zones);
    });
    render(<ZoneEditor workspace="ship" onClose={vi.fn()} />);
    const line = await screen.findByRole('separator', { name: 'Line between Code and Browser' });
    expect(line.getAttribute('aria-valuenow')).toBe('50');
    const save = screen.getByRole('button', { name: 'Save zones' });
    expect(save).toHaveProperty('disabled', true);
    fireEvent.keyDown(line, { key: 'ArrowRight', shiftKey: true });
    expect(screen.getByRole('separator', { name: 'Line between Code and Browser' }).getAttribute('aria-valuenow')).toBe('58');
    await userEvent.click(save);
    expect(saved).toMatchObject({
      cmd: 'set_zones',
      zones: [
        { id: 'z1', area: { right: 28 / 48 } },
        { id: 'z2', area: { left: 28 / 48 } },
      ],
    });
    expect(await screen.findByText('Zones of “Ship the release” saved.')).toBeTruthy();
  });

  it('splits, renames and removes a zone', async () => {
    engine.request.mockResolvedValue(zones);
    render(<ZoneEditor workspace="ship" onClose={vi.fn()} />);
    await userEvent.click(await screen.findByRole('button', { name: /^Code,/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Split side by side' }));
    expect(screen.getAllByRole('button', { pressed: false }).some((b) => b.textContent === 'Zone 1')).toBe(true);
    const name = screen.getByRole('textbox', { name: 'Name' });
    await userEvent.clear(name);
    expect(screen.getByRole('alert').textContent).toBe('Every zone needs a name.');
    await userEvent.type(name, 'Editor');
    await userEvent.click(screen.getByRole('button', { name: 'Remove zone' }));
    expect(screen.queryByRole('button', { name: /^Editor,/ })).toBeNull();
  });

  it('asks before leaving with changes', async () => {
    engine.request.mockResolvedValue(zones);
    const onClose = vi.fn();
    render(<ZoneEditor workspace="ship" onClose={onClose} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Use the preset: Thirds' }));
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onClose).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: 'Discard changes', hidden: true }));
    expect(onClose).toHaveBeenCalled();
  });
});
