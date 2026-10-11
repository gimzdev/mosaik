import { invoke } from '@tauri-apps/api/core';

import type { Command, Reply, Response } from './protocol';

/** Sends one command to the engine running in this process. */
export async function request(command: Command): Promise<Reply> {
  const response = await invoke<Response>('engine_request', { command });
  return response.reply;
}
