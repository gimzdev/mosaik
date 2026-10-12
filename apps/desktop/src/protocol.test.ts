import { describe, expect, it } from 'vitest';

import wire from '../../../crates/mosaik-ipc/fixtures/wire.json';
import { type Command, errorMessage, isReply } from './protocol';

// Every command the interface can send. Listing them as `Command['cmd']` makes TypeScript check
// each name; `AllListed` fails to compile when the union gains one that is not listed here.
const COMMANDS = [
  'hello',
  'state',
  'workspaces',
  'switch',
  'save',
  'rescue',
  'show_switcher',
  'show_settings',
  'quit',
  'settings',
  'set_settings',
  'check_hotkey',
  'zones',
  'set_zones',
  'rename_workspace',
  'delete_workspace',
  'set_workspace_hotkey',
  'hold_hotkeys',
] as const satisfies readonly Command['cmd'][];
type AllListed = Exclude<Command['cmd'], (typeof COMMANDS)[number]> extends never ? true : never;
const allListed: AllListed = true;

// `wire.json` is written by the Rust side (crates/mosaik-ipc/src/protocol.rs, `mod wire`).
describe('the engine protocol', () => {
  it('reads every reply the engine sends', () => {
    for (const reply of wire.replies) {
      expect(isReply(reply), JSON.stringify(reply)).toBe(true);
    }
  });

  it('knows every command the engine accepts, and no other', () => {
    expect(allListed).toBe(true);
    expect([...COMMANDS].sort()).toEqual(wire.commands.map((c) => c.cmd).sort());
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
  });
});
