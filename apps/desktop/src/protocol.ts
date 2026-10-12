// The engine's protocol (crates/mosaik-ipc/src/protocol.rs), as the interface sees it. The
// interface talks to the engine in process with the same commands mosaikctl sends over the pipe.
// `protocol.test.ts` checks these types against replies serialised by the Rust side.

export interface Rect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export type Command =
  | { cmd: 'hello'; client: string }
  | { cmd: 'state' }
  | { cmd: 'workspaces' }
  | { cmd: 'switch'; workspace: string }
  | { cmd: 'save'; workspace: string | null }
  | { cmd: 'rescue' }
  | { cmd: 'show_switcher' }
  | { cmd: 'show_settings' }
  | { cmd: 'quit' };

export type ErrorCode = 'unsupported_version' | 'bad_request' | 'not_found' | 'busy' | 'failed' | 'unknown';

export interface WorkspaceSummary {
  id: string;
  name: string;
  windows: number;
  zones: number;
  hotkey: string | null;
}

export interface MonitorState {
  id: string;
  name: string;
  primary: boolean;
  bounds: Rect;
  work_area: Rect;
  dpi: number;
  scale_percent: number;
}

export interface HotkeyState {
  shortcut: string;
  registered: boolean;
  problem: string | null;
}

export type Reply =
  | { reply: 'hello'; app_version: string; protocol: number }
  | { reply: 'workspaces'; workspaces: WorkspaceSummary[]; active: string | null }
  | { reply: 'done'; message: string }
  | { reply: 'error'; code: ErrorCode; message: string; hint: string | null }
  | {
      reply: 'state';
      app_version: string;
      protocol: number;
      safe_mode: boolean;
      watchdog: boolean;
      monitors: MonitorState[];
      hotkey: HotkeyState;
    };

/** Something the user should be told once. */
export interface Notice {
  title: string;
  body: string;
}

/** The reply's message when it is an error, for showing to the user. */
export function errorMessage(reply: Reply): string | null {
  if (reply.reply !== 'error') return null;
  return reply.hint ? `${reply.message} ${reply.hint}` : reply.message;
}

function isObject(x: unknown): x is Record<string, unknown> {
  return typeof x === 'object' && x !== null;
}

const isString = (x: unknown): x is string => typeof x === 'string';
const isNumber = (x: unknown): x is number => typeof x === 'number';
const isNullableString = (x: unknown): x is string | null => x === null || isString(x);

function isWorkspace(x: unknown): x is WorkspaceSummary {
  return (
    isObject(x) &&
    isString(x.id) &&
    isString(x.name) &&
    isNumber(x.windows) &&
    isNumber(x.zones) &&
    isNullableString(x.hotkey)
  );
}

/** Checks a reply's shape at run time, so a mismatch with the engine shows as an error. */
export function isReply(x: unknown): x is Reply {
  if (!isObject(x)) return false;
  switch (x.reply) {
    case 'hello':
      return isString(x.app_version) && isNumber(x.protocol);
    case 'workspaces':
      return Array.isArray(x.workspaces) && x.workspaces.every(isWorkspace) && isNullableString(x.active);
    case 'done':
      return isString(x.message);
    case 'error':
      return isString(x.code) && isString(x.message) && isNullableString(x.hint);
    case 'state':
      return isString(x.app_version) && typeof x.safe_mode === 'boolean' && Array.isArray(x.monitors);
    default:
      return false;
  }
}

const COMMANDS = new Set<Command['cmd']>([
  'hello',
  'state',
  'workspaces',
  'switch',
  'save',
  'rescue',
  'show_switcher',
  'show_settings',
  'quit',
]);

export function isCommand(x: unknown): x is Command {
  return isObject(x) && isString(x.cmd) && COMMANDS.has(x.cmd as Command['cmd']);
}
