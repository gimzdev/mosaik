// Sets of apps on the home page: the apps installed that go together for a task (coding: an
// editor, a terminal, a browser...), opened in one click. One line under Arrange, so the
// layouts stay in view: "Open the apps for Coding, Design, Writing"; each button names its apps
// to screen readers and, on hover, to the pointer. A set opens the workspace that has its apps,
// or becomes one laid out for its task; the apps that are not open start. Nothing shows when
// Mosaïk finds no set.

import { useEffect, useState } from 'react';

import { onWorkspacesChanged, request } from './engine';
import { type AppSet, errorMessage } from './protocol';
import { Button, useSay } from './ui';

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
    <div
      role="group"
      aria-labelledby="sets-title"
      className="mt-3 flex flex-wrap items-center justify-center gap-x-2 gap-y-1.5"
    >
      <span id="sets-title" className="text-small text-[var(--muted)]">
        Open the apps for
      </span>
      {sets.map((s) => {
        const apps = s.apps.join(', ');
        return (
          <Button
            key={s.id}
            label={`${s.name}: ${apps}`}
            hint={apps}
            pending={busy === s.id}
            disabled={busy !== null && busy !== s.id}
            onClick={() => {
              setBusy(s.id);
              void request({ cmd: 'open_set', set: s.id }).then((reply) => {
                setBusy(null);
                if (reply.reply === 'done') say(reply.message);
                else say(errorMessage(reply) ?? `The apps for ${s.name} could not be opened.`, true);
              });
            }}
          >
            {s.name}
          </Button>
        );
      })}
    </div>
  );
}
