import { invoke } from '@tauri-apps/api/core';

import { type Command, type Notice, type Reply, isReply } from './protocol';

/** Sends one command to the engine running in this process. */
export async function request(command: Command): Promise<Reply> {
  const reply: unknown = await invoke('engine_request', { command });
  if (isReply(reply)) return reply;
  return {
    reply: 'error',
    code: 'unknown',
    message: 'Mosaïk answered in a way this window does not understand.',
    hint: 'Reinstall Mosaïk so its parts match.',
  };
}

/** Takes the notices waiting to be shown (oldest first). */
export function takeNotices(): Promise<Notice[]> {
  return invoke<Notice[]>('take_notices');
}
