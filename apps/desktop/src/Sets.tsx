// Sets of apps on the home page: the apps installed that go together for a task (coding: an
// editor, a terminal, a browser...), opened in one click. A set becomes a workspace laid out for
// its task; the apps that are not open start. Nothing shows when Mosaïk finds no set.

import { useEffect, useState } from 'react';

import { onWorkspacesChanged, request } from './engine';
import { type AppSet, errorMessage } from './protocol';
import { useSay } from './ui';

export function Sets() {
  const [sets, setSets] = useState<AppSet[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const say = useSay();

  useEffect(() => {
    const load = () => {
      void request({ cmd: 'sets' }).then((reply) => {
        if (reply.reply === 'sets') setSets(reply.sets);
      });
    };
    load();
    return onWorkspacesChanged(load);
  }, []);

  if (!sets?.length) return null;
  return (
    <section aria-labelledby="sets-title" className="below-picture mt-8">
      <h2 id="sets-title" className="font-display text-heading font-semibold tracking-tight">
        Sets of apps
      </h2>
      <ul className="mt-3 grid grid-cols-[repeat(auto-fill,minmax(14rem,1fr))] gap-2.5">
        {sets.map((s) => (
          <li key={s.id}>
            <button
              type="button"
              aria-label={`Open ${s.name}: ${s.apps.join(', ')}`}
              aria-busy={busy === s.id || undefined}
              disabled={busy !== null}
              onClick={() => {
                setBusy(s.id);
                void request({ cmd: 'open_set', set: s.id }).then((reply) => {
                  setBusy(null);
                  if (reply.reply === 'done') say(reply.message);
                  else say(errorMessage(reply) ?? `“${s.name}” could not be opened.`, true);
                });
              }}
              className="flex w-full flex-col gap-0.5 rounded-xl border border-[var(--line)] bg-[var(--surface)] px-3 py-2 text-left transition-colors duration-150 hover:border-[var(--control)] disabled:opacity-60 forced-colors:border-[CanvasText]"
            >
              <span className="font-semibold">{s.name}</span>
              <span className="line-clamp-2 text-small text-[var(--muted)]">{s.apps.join(', ')}</span>
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
