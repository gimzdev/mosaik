import { describe, expect, it } from 'vitest';

import wire from '../../../crates/mosaik-ipc/fixtures/wire.json';
import { errorMessage, isCommand, isReply } from './protocol';

// `wire.json` is written by the Rust side (crates/mosaik-ipc/src/protocol.rs, `mod wire`).
describe('the engine protocol', () => {
  it('reads every reply the engine sends', () => {
    for (const reply of wire.replies) {
      expect(isReply(reply), JSON.stringify(reply)).toBe(true);
    }
  });

  it('knows every command the engine accepts', () => {
    for (const command of wire.commands) {
      expect(isCommand(command), JSON.stringify(command)).toBe(true);
    }
  });

  it('turns an error reply into one sentence with its hint', () => {
    const error = wire.replies.find((r) => r.reply === 'error');
    expect(isReply(error)).toBe(true);
    if (isReply(error)) {
      expect(errorMessage(error)).toBe('No workspace called "x". mosaikctl workspaces lists them.');
    }
  });

  it('rejects shapes it does not know', () => {
    expect(isReply({ v: 1, id: 0, reply: 'workspaces' })).toBe(false);
    expect(isReply({ reply: 'teleport' })).toBe(false);
    expect(isCommand({ cmd: 'format_disk' })).toBe(false);
  });
});
