// The interface's way to Mosaïk: engine commands, notices, events, and the few things only the
// app itself does (links, the logs folder, a rescue that works without the engine, the title
// bar). In a plain browser during development a stand-in engine answers instead.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { type Command, type Notice, type Reply, isReply } from './protocol';

/** Inside Mosaïk (Tauri), as opposed to a plain browser during development. */
const inMosaik = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

const notInMosaik: Reply = {
  reply: 'error',
  code: 'failed',
  message: 'This window only works inside Mosaïk.',
  hint: null,
};

/** Sends one command to the engine running in this process. */
export async function request(command: Command): Promise<Reply> {
  if (!inMosaik) {
    // The development server answers with a stand-in engine (never part of a release build).
    if (import.meta.env.DEV) {
      const { mock } = await import('./mock');
      return mock.request(command);
    }
    return notInMosaik;
  }
  try {
    const reply: unknown = await invoke('engine_request', { command });
    if (isReply(reply)) return reply;
  } catch (e) {
    return {
      reply: 'error',
      code: 'failed',
      message: `Mosaïk did not answer: ${String(e)}`,
      hint: 'Restart Mosaïk from the Start menu.',
    };
  }
  return {
    reply: 'error',
    code: 'unknown',
    message: 'Mosaïk answered in a way this window does not understand.',
    hint: 'Reinstall Mosaïk so its parts match.',
  };
}

/** Takes the notices waiting to be shown (oldest first). */
export async function takeNotices(): Promise<Notice[]> {
  if (!inMosaik) return [];
  return invoke<Notice[]>('take_notices');
}

function on(event: string, callback: () => void): () => void {
  if (!inMosaik) {
    // The stand-in engine of the development server tells its changes with window events.
    if (!import.meta.env.DEV || typeof window === 'undefined') return () => undefined;
    const name = `mosaik:${event}`;
    window.addEventListener(name, callback);
    return () => {
      window.removeEventListener(name, callback);
    };
  }
  const stop = listen(event, callback);
  return () => {
    void stop.then((f) => {
      f();
    });
  };
}

/** Calls `callback` whenever new notices arrive; returns a function that stops listening. */
export function onNotices(callback: () => void): () => void {
  return on('notices:changed', callback);
}

/** The workspace whose zones the quick menu asked to edit, if any (each ask is taken once). */
export async function takeZonesToEdit(): Promise<string | null> {
  if (!inMosaik) return null;
  return invoke<string | null>('take_zones_to_edit');
}

/** Calls `callback` whenever the quick menu asks to edit a workspace's zones. */
export function onZonesToEdit(callback: () => void): () => void {
  return on('zones:edit', callback);
}

/** Calls `callback` whenever the saved workspaces, or the one open, change. */
export function onWorkspacesChanged(callback: () => void): () => void {
  return on('workspaces:changed', callback);
}

/** Runs `action`, turning a failure of the app itself into a message. */
async function attempt(action: () => Promise<unknown>, failed: string): Promise<string | null> {
  try {
    await action();
    return null;
  } catch (e) {
    return typeof e === 'string' ? e : `${failed} ${String(e)}`;
  }
}

/** The pages Mosaïk links to. Only these open, in the default browser. */
export type Link = 'patreon' | 'site' | 'source' | 'privacy';

/** Opens a page in the default browser; resolves to an error message, or null. */
export function openLink(link: Link): Promise<string | null> {
  if (!inMosaik) return Promise.resolve(null);
  return attempt(() => invoke('open_link', { link }), 'The browser did not open.');
}

/** Opens the licences of the open-source software Mosaïk is built with (a text file beside
 * mosaik.exe); resolves to an error message, or null. */
export function openLicences(): Promise<string | null> {
  if (!inMosaik) return Promise.resolve(null);
  return attempt(() => invoke('open_licences'), 'The list of licences did not open.');
}

/** Opens the folder with Mosaïk's logs in File Explorer; resolves to an error message, or null. */
export function openLogs(): Promise<string | null> {
  if (!inMosaik) return Promise.resolve(null);
  return attempt(() => invoke('open_logs'), 'The logs folder did not open.');
}

/** Brings back every window Mosaïk hid, even when its engine does not answer. */
export async function rescueWindows(): Promise<{ text: string; error: boolean }> {
  if (!inMosaik) {
    const reply = await request({ cmd: 'rescue' });
    return reply.reply === 'done'
      ? { text: reply.message, error: false }
      : { text: 'Rescue did not finish.', error: true };
  }
  try {
    return { text: await invoke<string>('rescue_windows'), error: false };
  } catch (e) {
    return { text: String(e), error: true };
  }
}

/** Matches the window's title bar to the page: dark or light, in the page's colours. Without
 * colours (high contrast), Windows' own stay. A title bar left in Windows' colours is not worth
 * a message, so a failure here is ignored. */
export function styleTitleBar(look: { dark: boolean; caption: string; text: string } | null): void {
  if (!inMosaik) return;
  void invoke('style_title_bar', {
    dark: look?.dark ?? null,
    caption: look?.caption ?? null,
    text: look?.text ?? null,
  }).catch(() => undefined);
}

/** Until the returned function is called, closing the window first runs `ask` (unsaved zone
 * edits), which shows the question and resolves to whether to close anyway. */
export function guardClose(ask: () => Promise<boolean>): () => void {
  if (!inMosaik) return () => undefined;
  let stop: (() => void) | null = null;
  let stopped = false;
  const window = getCurrentWindow();
  void window
    .onCloseRequested(async (event) => {
      event.preventDefault();
      if (await ask()) await window.destroy();
    })
    .then((f) => {
      if (stopped) f();
      else stop = f;
    });
  return () => {
    stopped = true;
    stop?.();
  };
}
