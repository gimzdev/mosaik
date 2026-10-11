// The engine's protocol (crates/mosaik-ipc/src/protocol.rs), as the interface sees it. The
// interface talks to the engine in process with the same commands mosaikctl sends over the pipe.

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
      monitors: MonitorState[];
      hotkey: HotkeyState;
    };

export interface Response {
  v: number;
  id: number;
  reply: Reply;
}

/** The reply's message when it is an error, for showing to the user. */
export function errorMessage(reply: Reply): string | null {
  if (reply.reply !== 'error') return null;
  return reply.hint ? `${reply.message} ${reply.hint}` : reply.message;
}
