import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Command, Reply, ZoneEdit } from './protocol';
import { Feedback } from './ui';
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
  zones,
  revision,
});

const two: ZoneEdit[] = [
  { id: 'z1', name: 'Code', color: 'lapis', monitor: 'm1', area: { left: 0, top: 0, right: 0.5, bottom: 1 } },
  { id: 'z2', name: 'Browser', color: 'verdigris', monitor: 'm1', area: { left: 0.5, top: 0, right: 1, bottom: 1 } },
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
    <Feedback>
      <ZoneEditor workspace="ship" onClose={onClose} onDirtyChange={onDirtyChange} />
    </Feedback>,
  );
  return { onDirtyChange, onClose };
}

describe('the zone editor', () => {
  it('moves a line from the keyboard and saves', async () => {
    const sent = engineWithZones();
    const { onDirtyChange } = editor();
    const line = await screen.findByRole('separator', { name: 'Line between Code and Browser' });
    expect(line.getAttribute('aria-valuenow')).toBe('50');
    expect(line.getAttribute('aria-valuemin')).toBe('5');
    expect(line.getAttribute('aria-valuemax')).toBe('95');
    const save = screen.getByRole('button', { name: 'Save zones' });
    expect(save).toHaveProperty('disabled', true);
    fireEvent.keyDown(line, { key: 'ArrowRight', shiftKey: true });
    expect(screen.getByRole('separator', { name: 'Line between Code and Browser' }).getAttribute('aria-valuenow')).toBe(
      '58',
    );
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
    expect(await screen.findByText('Zones saved.')).toBeTruthy();
    expect(onDirtyChange).toHaveBeenLastCalledWith(false);
  });

  it('keeps the ids of new zones once saved, so saving again does not replace them', async () => {
    const sent = engineWithZones();
    editor();
    await userEvent.click(await screen.findByRole('button', { name: /^Code,/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Split side by side' }));
    await userEvent.click(screen.getByRole('button', { name: 'Save zones' }));
    await screen.findByText('Zones saved.');
    const line = screen.getByRole('separator', { name: 'Line between Code and Zone 1' });
    fireEvent.keyDown(line, { key: 'ArrowLeft' });
    await userEvent.click(screen.getByRole('button', { name: 'Save zones' }));
    const saves = sent.filter((c) => c.cmd === 'set_zones');
    expect(saves).toHaveLength(2);
    expect(saves[1]).toMatchObject({ revision: '2' });
    expect(saves[1]?.cmd === 'set_zones' && saves[1].zones.every((z) => z.id !== null)).toBe(true);
  });

  it('renames, recolours, splits and removes a zone', async () => {
    engine.request.mockResolvedValue(reply(two));
    editor();
    await userEvent.click(await screen.findByRole('button', { name: /^Code,/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Split top and bottom' }));
    expect(screen.getAllByRole('button', { pressed: false }).some((b) => b.textContent === 'Zone 1')).toBe(true);
    await userEvent.click(screen.getByRole('radio', { name: 'Saffron' }));
    const name = screen.getByRole('textbox', { name: 'Name' });
    await userEvent.clear(name);
    expect(screen.getByText('Every zone needs a name.').getAttribute('role')).toBe('alert');
    expect(screen.getByRole('button', { name: 'Save zones' })).toHaveProperty('disabled', true);
    await userEvent.type(name, 'Editor');
    await userEvent.click(screen.getByRole('button', { name: 'Remove zone' }));
    expect(screen.queryByRole('button', { name: /^Editor,/ })).toBeNull();
  });

  it('asks before leaving with changes', async () => {
    engine.request.mockResolvedValue(reply(two));
    const { onClose } = editor();
    await userEvent.click(await screen.findByRole('button', { name: 'Start from a preset' }));
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
    await userEvent.click(screen.getByRole('button', { name: 'Back to workspaces' }));
    expect(onClose).toHaveBeenCalled();
  });
});
