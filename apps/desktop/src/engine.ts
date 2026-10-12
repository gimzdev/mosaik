import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

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

/** Calls `onChange` whenever new notices arrive; returns a function that stops listening. */
export function onNotices(onChange: () => void): () => void {
  if (!inMosaik) return () => undefined;
  const stop = listen('notices:changed', onChange);
  return () => {
    void stop.then((f) => {
      f();
    });
  };
}

/** The pages Mosaïk links to. Only these open, in the default browser. */
export type Link = 'patreon' | 'site' | 'source' | 'privacy';

export async function openLink(link: Link): Promise<void> {
  if (inMosaik) await invoke('open_link', { link });
}

/** Opens the folder with Mosaïk's logs in File Explorer. */
export async function openLogs(): Promise<void> {
  if (inMosaik) await invoke('open_logs');
}
