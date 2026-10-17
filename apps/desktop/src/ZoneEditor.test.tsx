import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Command, Reply, ZoneEdit } from './protocol';
import { shown } from './test-fixtures';
import { Announcer } from './ui';
import { ZoneEditor } from './ZoneEditor';

const engine = vi.hoisted(() => ({ request: vi.fn<(c: Command) => Promise<Reply>>() }));
vi.mock('./engine', () => engine);

afterEach(cleanup);
beforeEach(() => {
  engine.request.mockReset();
});

const reply = (zones: ZoneEdit[], revision = '1'): Reply => ({
  reply: 'zones',
  workspace: 'ship',
  name: 'Web development',
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
  zones,
  revision,
});

const two: ZoneEdit[] = [
  { id: 'z1', name: '1', color: 'lapis', monitor: 'm1', area: { left: 0, top: 0, right: 0.5, bottom: 1 } },
  { id: 'z2', name: '2', color: 'verdigris', monitor: 'm1', area: { left: 0.5, top: 0, right: 1, bottom: 1 } },
];

/** An engine that keeps what it is sent, giving new zones ids as Mosaïk does. */
function engineWithZones() {
  let zones = two;
  let revision = 1;
  const sent: Command[] = [];
  engine.request.mockImplementation((c) => {
    sent.push(c);
    if (c.cmd === 'set_zones') {
      if (c.revision !== String(revision)) {
        return Promise.resolve({ reply: 'error', code: 'bad_request', message: 'Stale.', hint: null });
      }
      let n = 2;
      zones = c.zones.map((z) => (z.id ? z : { ...z, id: `z${String((n += 1))}` }));
      revision += 1;
    }
    return Promise.resolve(reply(zones, String(revision)));
  });
  return sent;
}

function editor(onDirtyChange = vi.fn(), onClose = vi.fn()) {
  render(
    <Announcer>
      <ZoneEditor workspace="ship" onClose={onClose} onDirtyChange={onDirtyChange} />
    </Announcer>,
  );
  return { onDirtyChange, onClose };
}

describe('the zone editor', () => {
  it('moves a line from the keyboard and saves', async () => {
    const sent = engineWithZones();
    const { onDirtyChange } = editor();
    const line = await screen.findByRole('separator', { name: 'Line between zone 1 and zone 2' });
    expect(line.getAttribute('aria-valuenow')).toBe('50');
    expect(line.getAttribute('aria-valuemin')).toBe('5');
    expect(line.getAttribute('aria-valuemax')).toBe('95');
    const save = screen.getByRole('button', { name: 'Save zones' });
    expect(save).toHaveProperty('disabled', true);
    fireEvent.keyDown(line, { key: 'ArrowRight', shiftKey: true });
    expect(
      screen.getByRole('separator', { name: 'Line between zone 1 and zone 2' }).getAttribute('aria-valuenow'),
    ).toBe('58');
    expect(onDirtyChange).toHaveBeenLastCalledWith(true);
    await userEvent.click(save);
    expect(sent.find((c) => c.cmd === 'set_zones')).toMatchObject({
      cmd: 'set_zones',
      revision: '1',
      zones: [
        { id: 'z1', area: { right: 28 / 48 } },
        { id: 'z2', area: { left: 28 / 48 } },
      ],
    });
    expect(await shown('Zones saved.')).toBeTruthy();
    expect(onDirtyChange).toHaveBeenLastCalledWith(false);
  });

  it('keeps the ids of new zones once saved, so saving again does not replace them', async () => {
    const sent = engineWithZones();
    editor();
    await userEvent.click(await screen.findByRole('button', { name: /^Zone 1,/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Split side by side' }));
    await userEvent.click(screen.getByRole('button', { name: 'Save zones' }));
    await shown('Zones saved.');
    const line = screen.getByRole('separator', { name: 'Line between zone 1 and zone 2' });
    fireEvent.keyDown(line, { key: 'ArrowLeft' });
    await userEvent.click(screen.getByRole('button', { name: 'Save zones' }));
    const saves = sent.filter((c) => c.cmd === 'set_zones');
    expect(saves).toHaveLength(2);
    expect(saves[1]).toMatchObject({ revision: '2' });
    expect(saves[1]?.cmd === 'set_zones' && saves[1].zones.every((z) => z.id !== null)).toBe(true);
  });

  it('numbers zones by place as they are split and removed, and recolours them', async () => {
    // The owner: zone names "should be 1 2 3 4 5 6". Zones have no name to type.
    engine.request.mockResolvedValue(reply(two));
    editor();
    await userEvent.click(await screen.findByRole('button', { name: /^Zone 1,/ }));
    expect(screen.getByRole('region', { name: 'Zone 1' })).toBeInTheDocument();
    expect(screen.queryByRole('textbox')).toBeNull();
    // Split, the left half's new bottom reads last: 1 at the top left, 2 on the right, 3 below.
    await userEvent.click(screen.getByRole('button', { name: 'Split top and bottom' }));
    const zones = () => screen.getAllByRole('button', { name: /^Zone \d,/ }).map((b) => b.getAttribute('aria-label'));
    expect(zones()).toEqual([
      'Zone 1, 50% wide, 50% high',
      'Zone 3, 50% wide, 50% high',
      'Zone 2, 50% wide, 100% high',
    ]);
    await userEvent.click(screen.getByRole('radio', { name: 'Saffron' }));
    expect(screen.getByRole('button', { name: 'Save zones' })).toHaveProperty('disabled', false);
    await userEvent.click(screen.getByRole('button', { name: 'Remove zone' }));
    // The zone on the right reads first now.
    expect(zones()).toEqual(['Zone 2, 50% wide, 50% high', 'Zone 1, 50% wide, 100% high']);
  });

  it('asks before leaving with changes', async () => {
    engine.request.mockResolvedValue(reply(two));
    const { onClose } = editor();
    await userEvent.click(await screen.findByRole('button', { name: 'Start from a layout' }));
    await userEvent.click(screen.getByRole('button', { name: 'Thirds' }));
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onClose).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: 'Discard changes', hidden: true }));
    expect(onClose).toHaveBeenCalled();
  });

  it('says why the zones could not be read, with a way back', async () => {
    engine.request.mockResolvedValue({
      reply: 'error',
      code: 'not_found',
      message: 'There is no workspace called “ship”.',
      hint: null,
    });
    const { onClose } = editor();
    expect((await screen.findByText('There is no workspace called “ship”.')).getAttribute('role')).toBe('alert');
    await userEvent.click(screen.getByRole('button', { name: 'Back' }));
    expect(onClose).toHaveBeenCalled();
  });
});
