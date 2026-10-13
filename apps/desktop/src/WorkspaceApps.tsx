// A workspace's apps, under its row on the Workspaces page: each with the zone it goes in (a
// list changes it), Remove to take it out, and the windows on screen to add. In the open
// workspace a change moves the window at once; the engine answers each one with the list as it
// is now.

import { useCallback, useEffect, useState } from 'react';

import { onWorkspacesChanged, request } from './engine';
import { type Command, type WorkspaceApp, type WorkspaceApps, errorMessage } from './protocol';
import { Button, MenuButton, useSay } from './ui';

export function AppsPanel({ workspace, id }: { workspace: string; id: string }) {
  const [data, setData] = useState<WorkspaceApps | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const say = useSay();

  const load = useCallback(() => {
    void request({ cmd: 'apps', workspace }).then((reply) => {
      if (reply.reply === 'apps') {
        setData(reply);
        setProblem(null);
      } else {
        setProblem(errorMessage(reply) ?? 'Its apps could not be read.');
      }
    });
  }, [workspace]);

  // Windows open and close meanwhile: the list follows when Mosaïk's window comes back.
  useEffect(() => {
    load();
    window.addEventListener('focus', load);
    const stop = onWorkspacesChanged(load);
    return () => {
      window.removeEventListener('focus', load);
      stop();
    };
  }, [load]);

  const change = (key: string, command: Command, done: string) => {
    setBusy(key);
    void request(command).then((reply) => {
      setBusy(null);
      if (reply.reply === 'apps') {
        setData(reply);
        say(done);
      } else {
        say(errorMessage(reply) ?? 'The workspace could not be changed.', true);
        load();
      }
    });
  };

  const zoneOf = (a: WorkspaceApp) => data?.zones.find((z) => z.id === a.zone) ?? null;

  return (
    <div id={id} className="mt-3 border-t border-[var(--line)] pt-2">
      {problem ? (
        <p role="alert" className="py-1 text-small text-[var(--danger)]">
          {problem}
        </p>
      ) : data === null ? (
        <p className="py-1 text-small text-[var(--muted)]" aria-busy="true">
          Reading its apps…
        </p>
      ) : (
        <>
          {data.apps.length === 0 ? (
            <p className="py-1 text-small text-[var(--muted)]">No apps yet.</p>
          ) : (
            <ul aria-label={`Apps of “${data.name}”`} className="flex flex-col">
              {data.apps.map((a) => {
                const zone = zoneOf(a);
                return (
                  <li key={a.id} className="flex items-center gap-3 py-1">
                    <span
                      aria-hidden="true"
                      className="size-2.5 shrink-0 rounded-full forced-colors:bg-[CanvasText]"
                      style={{ background: zone ? `var(--zone-${zone.color})` : 'var(--control)' }}
                    />
                    <span className="min-w-0 flex-1 truncate font-semibold">{a.label}</span>
                    {data.zones.length ? (
                      <select
                        aria-label={`Zone of ${a.label}`}
                        value={a.zone ?? ''}
                        disabled={busy !== null}
                        onChange={(e) => {
                          const to = data.zones.find((z) => z.id === e.target.value);
                          if (!to) return;
                          change(
                            a.id,
                            { cmd: 'move_app', workspace: data.workspace, app: a.id, zone: to.id },
                            `${a.label} goes in ${to.name}.`,
                          );
                        }}
                        className="field w-40 shrink-0 text-small"
                      >
                        {a.zone === null ? (
                          <option value="" disabled>
                            Its own place
                          </option>
                        ) : null}
                        {data.zones.map((z) => (
                          <option key={z.id} value={z.id}>
                            {z.name}
                          </option>
                        ))}
                      </select>
                    ) : null}
                    <Button
                      tone="quiet"
                      label={`Remove ${a.label}`}
                      pending={busy === a.id}
                      disabled={busy !== null && busy !== a.id}
                      onClick={() => {
                        change(
                          a.id,
                          { cmd: 'remove_app', workspace: data.workspace, app: a.id },
                          `${a.label} removed.`,
                        );
                      }}
                    >
                      Remove
                    </Button>
                  </li>
                );
              })}
            </ul>
          )}
          <div className="mt-1 flex items-center">
            {data.windows.length ? (
              <MenuButton
                label={`Add a window to “${data.name}”`}
                quiet
                align="left"
                items={data.windows.map((w) => ({
                  key: String(w.window),
                  label: w.label,
                  detail: w.title && w.title !== w.label ? w.title : undefined,
                  onSelect: () => {
                    change(
                      'add',
                      { cmd: 'add_app', workspace: data.workspace, window: w.window, zone: null },
                      `${w.label} added.`,
                    );
                  },
                }))}
              >
                <span aria-hidden="true">+</span> Add a window
              </MenuButton>
            ) : (
              <p className="py-1 text-small text-[var(--muted)]">Open a window to add it here.</p>
            )}
          </div>
        </>
      )}
    </div>
  );
}
