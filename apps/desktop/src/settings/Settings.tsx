import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';

import { request, takeNotices } from '../engine';
import { errorMessage, type Notice } from '../protocol';
import { Notices } from './Notices';

/** The settings window. Grows into workspaces, zones, shortcuts and updates in step 6. */
export function Settings() {
  const [version, setVersion] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [notices, setNotices] = useState<Notice[]>([]);

  useEffect(() => {
    // Notices wait in the app until a window takes them: now, and whenever more arrive.
    const collect = () => {
      void takeNotices().then((fresh) => {
        if (fresh.length) setNotices((n) => [...n, ...fresh]);
      });
    };
    void request({ cmd: 'hello', client: 'settings' }).then((reply) => {
      if (reply.reply === 'hello') setVersion(reply.app_version);
      else setProblem(errorMessage(reply) ?? 'Mosaïk did not answer.');
    });
    collect();
    const unlisten = listen('notices:changed', collect);
    return () => {
      void unlisten.then((stop) => {
        stop();
      });
    };
  }, []);

  return (
    <main className="mx-auto max-w-2xl p-8">
      <h1 className="font-display text-3xl font-bold tracking-tight">Mosaïk</h1>
      <p className="mt-2 text-[var(--muted)]">{version ? `Version ${version}` : (problem ?? 'Starting…')}</p>
      <Notices
        notices={notices}
        onDismiss={(index) => {
          setNotices((n) => n.filter((_, i) => i !== index));
        }}
      />
    </main>
  );
}
