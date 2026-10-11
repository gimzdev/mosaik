import { useEffect, useState } from 'react';

import { request } from '../engine';

/** The settings window. Grows into workspaces, zones, shortcuts and updates in step 6. */
export function Settings() {
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    void request({ cmd: 'hello', client: 'settings' }).then((reply) => {
      if (reply.reply === 'hello') setVersion(reply.app_version);
    });
  }, []);

  return (
    <main className="mx-auto max-w-2xl p-8">
      <h1 className="font-display text-3xl font-bold tracking-tight">Mosaïk</h1>
      <p className="mt-2 text-[var(--muted)]">{version ? `Version ${version}` : 'Starting…'}</p>
    </main>
  );
}
