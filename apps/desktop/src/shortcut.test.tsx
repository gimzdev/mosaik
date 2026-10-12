import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Command, Reply } from './protocol';
import { keyName, record, ShortcutRecorder } from './shortcut';

const engine = vi.hoisted(() => ({
  request: vi.fn<(c: Command) => Promise<Reply>>((command) =>
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
vi.mock('./engine', () => engine);

afterEach(cleanup);
beforeEach(() => {
  engine.request.mockClear();
});

const press = { keyCode: 0, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, altGraph: false };

describe('recording a shortcut', () => {
  it('names keys as Windows registers them', () => {
    // An AZERTY keyboard: the key labelled A sits where QWERTY has Q.
    expect(keyName('a', 'KeyQ', 65)).toBe('A');
    // AZERTY digits type symbols without Shift; the key is still the digit.
    expect(keyName('&', 'Digit1', 49)).toBe('1');
    expect(keyName(' ', 'Space', 32)).toBe('Space');
    expect(keyName('ArrowLeft', 'ArrowLeft', 37)).toBe('Left');
    expect(keyName('F13', 'F13', 124)).toBe('F13');
    // German "ü" is where a US keyboard has "[", but Windows knows it as VK_OEM_1.
    expect(keyName('ü', 'BracketLeft', 0xba)).toBe('Semicolon');
    expect(keyName('Unidentified', 'Lang1', 0)).toBeNull();
  });

  it('builds the shortcut from the modifiers held', () => {
    expect(record({ ...press, key: ' ', code: 'Space', keyCode: 32, metaKey: true, altKey: true })).toEqual({
      shortcut: 'Win+Alt+Space',
    });
    expect(record({ ...press, key: 'Shift', code: 'ShiftLeft', shiftKey: true })).toBeNull();
  });

  it('refuses AltGr with a key that types, and takes it as Ctrl+Alt otherwise', () => {
    const typing = record({
      ...press,
      key: '€',
      code: 'KeyE',
      keyCode: 69,
      ctrlKey: true,
      altKey: true,
      altGraph: true,
    });
    expect(typing).toEqual({ problem: expect.stringContaining('AltGr') as unknown });
    expect(record({ ...press, key: 'F5', code: 'F5', keyCode: 116, altGraph: true })).toEqual({
      shortcut: 'Ctrl+Alt+F5',
    });
  });

  it('checks the shortcut with Mosaïk before it can be saved', async () => {
    const onSave = vi.fn(() => Promise.resolve(null));
    render(<ShortcutRecorder label="Open the switcher" value="Win+Alt+Space" onSave={onSave} />);
    await userEvent.click(screen.getByRole('button', { name: 'Change: Open the switcher' }));
    const box = await screen.findByRole('button', { name: 'Press the new shortcut' });
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'hold_hotkeys', hold: true });
    fireEvent.keyDown(box, { key: 'k', code: 'KeyK', keyCode: 75, ctrlKey: true, altKey: true });
    expect((await screen.findByRole('alert')).textContent).toBe('Ctrl+Alt is AltGr on many keyboards.');
    expect(screen.getByRole('button', { name: 'Save shortcut' })).toHaveProperty('disabled', true);
    fireEvent.keyDown(box, { key: 'm', code: 'KeyM', keyCode: 77, metaKey: true, altKey: true });
    await screen.findByRole('button', { name: 'New shortcut: Win+Alt+M' });
    const save = screen.getByRole('button', { name: 'Save shortcut' });
    expect(save).toHaveProperty('disabled', false);
    await userEvent.click(save);
    expect(onSave).toHaveBeenCalledWith('Win+Alt+M');
    expect(engine.request).toHaveBeenCalledWith({ cmd: 'hold_hotkeys', hold: false });
  });

  it('can be used with the keyboard alone', async () => {
    const onSave = vi.fn(() => Promise.resolve(null));
    render(<ShortcutRecorder label="Open the switcher" value="Win+Alt+Space" onSave={onSave} />);
    screen.getByRole('button', { name: 'Change: Open the switcher' }).focus();
    await userEvent.keyboard('{Enter}');
    const box = await screen.findByRole('button', { name: 'Press the new shortcut' });
    await waitFor(() => {
      expect(document.activeElement).toBe(box);
    });
    await userEvent.keyboard('{Meta>}{Alt>}m{/Alt}{/Meta}');
    await screen.findByRole('button', { name: 'New shortcut: Win+Alt+M' });
    // Tab leaves the box for Save, instead of being recorded.
    await userEvent.tab();
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Save shortcut' }));
    await userEvent.keyboard('{Enter}');
    expect(onSave).toHaveBeenCalledWith('Win+Alt+M');
    // Focus goes back to the button that started it.
    await waitFor(() => {
      expect(document.activeElement?.textContent).toBe('Change');
    });
  });
});
