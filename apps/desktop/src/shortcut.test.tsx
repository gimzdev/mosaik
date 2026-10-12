import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { keyName, record, ShortcutRecorder } from './shortcut';

vi.mock('./engine', () => ({
  request: vi.fn((command: { cmd: string; hotkey?: string }) =>
    Promise.resolve(
      command.cmd === 'check_hotkey'
        ? {
            reply: 'hotkey',
            hotkey: command.hotkey,
            problem: command.hotkey === 'Ctrl+Alt+K' ? 'Ctrl+Alt is AltGr on many keyboards.' : null,
            warnings: [],
          }
        : { reply: 'done', message: 'ok' },
    ),
  ),
}));

afterEach(cleanup);

const press = { metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, altGraph: false };

describe('recording a shortcut', () => {
  it('names keys as Mosaïk writes them', () => {
    // An AZERTY keyboard: the key labelled A sits where QWERTY has Q.
    expect(keyName('a', 'KeyQ')).toBe('A');
    // AZERTY digits type symbols without Shift; the key is still the digit.
    expect(keyName('&', 'Digit1')).toBe('1');
    expect(keyName(' ', 'Space')).toBe('Space');
    expect(keyName('ArrowLeft', 'ArrowLeft')).toBe('Left');
    expect(keyName('F13', 'F13')).toBe('F13');
    expect(keyName('Unidentified', 'Lang1')).toBeNull();
  });

  it('builds the shortcut from the modifiers held', () => {
    expect(record({ ...press, key: ' ', code: 'Space', metaKey: true, altKey: true })).toEqual({ shortcut: 'Win+Alt+Space' });
    expect(record({ ...press, key: 'Shift', code: 'ShiftLeft', shiftKey: true })).toBeNull();
  });

  it('refuses AltGr, which types characters', () => {
    const r = record({ ...press, key: '€', code: 'KeyE', ctrlKey: true, altKey: true, altGraph: true });
    expect(r).toEqual({ problem: expect.stringContaining('AltGr') as unknown });
  });

  it('checks the shortcut with Mosaïk before it can be saved', async () => {
    const onSave = vi.fn(() => Promise.resolve(null));
    render(<ShortcutRecorder label="Open the switcher" value="Win+Alt+Space" onSave={onSave} />);
    await userEvent.click(screen.getByRole('button', { name: 'Change the shortcut: Open the switcher' }));
    const box = await screen.findByRole('button', { name: 'Press the new shortcut' });
    fireEvent.keyDown(box, { key: 'k', code: 'KeyK', ctrlKey: true, altKey: true });
    expect(await screen.findByRole('alert')).toHaveProperty('textContent', 'Ctrl+Alt is AltGr on many keyboards.');
    expect(screen.getByRole('button', { name: 'Save shortcut' })).toHaveProperty('disabled', true);
    fireEvent.keyDown(box, { key: 'm', code: 'KeyM', metaKey: true, altKey: true });
    await screen.findByRole('button', { name: 'New shortcut: Win+Alt+M' });
    const save = screen.getByRole('button', { name: 'Save shortcut' });
    expect(save).toHaveProperty('disabled', false);
    await userEvent.click(save);
    expect(onSave).toHaveBeenCalledWith('Win+Alt+M');
    expect(await screen.findByText('Win+Alt+M saved.')).toBeTruthy();
  });
});
