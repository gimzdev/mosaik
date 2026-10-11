import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useState } from 'react';

import { request } from '../engine';
import { errorMessage, type WorkspaceSummary } from '../protocol';
import { Switcher } from './Switcher';

interface OpenEvent {
  /** Matches the open request, so the engine can time how long the switcher took to appear. */
  token: number;
  shortcut: string;
}

/** The switcher window: created hidden at start, shown by the engine when the hotkey fires. */
export function SwitcherWindow() {
  const [workspaces, setWorkspaces] = useState<WorkspaceSummary[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [shortcut, setShortcut] = useState('Win+Alt+Space');
  const [problem, setProblem] = useState<string | null>(null);
  // A new key remounts the switcher on every open, so it starts unfiltered with focus in it.
  const [openCount, setOpenCount] = useState(0);

  const refresh = useCallback(async () => {
    const reply = await request({ cmd: 'workspaces' });
    if (reply.reply === 'workspaces') {
      setWorkspaces(reply.workspaces);
      setActive(reply.active);
      setProblem(null);
    } else {
      setProblem(errorMessage(reply));
    }
  }, []);

  useEffect(() => {
    const unlisten = listen<OpenEvent>('switcher:open', (event) => {
      setShortcut(event.payload.shortcut);
      setOpenCount((n) => n + 1);
      void refresh();
      // Two frames: the first after React commits, the second once it is on screen.
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          void invoke('switcher_shown', { token: event.payload.token });
        });
      });
    });
    return () => {
      void unlisten.then((stop) => {
        stop();
      });
    };
  }, [refresh]);

  const close = useCallback(() => {
    void invoke('switcher_hide');
  }, []);

  return (
    <div className="h-full p-3">
      <Switcher
        key={openCount}
        workspaces={workspaces}
        active={active}
        shortcut={shortcut}
        onOpen={(w) => {
          void request({ cmd: 'switch', workspace: w.id }).then((reply) => {
            const message = errorMessage(reply);
            if (message) setProblem(message);
            else close();
          });
        }}
        onSave={() => {
          void request({ cmd: 'save', workspace: null }).then((reply) => {
            setProblem(errorMessage(reply));
            void refresh();
          });
        }}
        onClose={close}
      />
      {problem && (
        <p role="alert" className="mt-2 rounded-lg bg-[#3a1d26] px-3 py-2 text-[13px] text-[#ffd7df]">
          {problem}
        </p>
      )}
    </div>
  );
}
