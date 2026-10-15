// Reading what a page shows from Mosaïk, a read at a time. Each read is a scan of every window in
// the engine (10 ms or more), and the engine serves one at a time: a burst of reads (the window
// coming back into focus, the workspaces changing, the page's own change, often together) would
// hold up drags and shortcuts. Here a burst becomes one read, about 150 ms after the last of them,
// and a read never starts while another is under way.

import { useCallback, useEffect, useLayoutEffect, useRef } from 'react';

import { onWorkspacesChanged } from './engine';

/** Why a read is asked: the page opened or what it shows changed (`start`), Mosaïk's window came
 * back into focus (`focus`), Mosaïk said the workspaces changed (`changed`), or the page changed
 * something itself and has the answer already (`own`). */
export type Reason = 'start' | 'focus' | 'changed' | 'own';

/** How long a read waits after the last thing that asked for it, in milliseconds. */
export const SETTLE_MS = 150;

/** A read: it is told why it was asked (everything since the last one), and whether its answer is
 * still wanted (the page is open, and shows what it asked about). */
export type Read = (why: ReadonlySet<Reason>, current: () => boolean) => Promise<void>;

interface State {
  alive: boolean;
  /** Goes up whenever what the page shows changes: an answer to an older read is dropped. */
  generation: number;
  running: boolean;
  /** Asked for while a read was under way: one more read follows it. */
  again: boolean;
  why: Set<Reason>;
  timer: ReturnType<typeof setTimeout> | null;
}

/**
 * Runs `read` at once when the page opens and whenever `what` (what the page shows) changes;
 * then once things settle after the window comes back into focus, the workspaces change, or the
 * page calls the function returned with its own reason.
 */
export function useReads(read: Read, what: string): (why: Reason) => void {
  const latest = useRef(read);
  useLayoutEffect(() => {
    latest.current = read;
  });
  const state = useRef<State>({
    alive: false,
    generation: 0,
    running: false,
    again: false,
    why: new Set(),
    timer: null,
  });

  const run = useCallback(() => {
    const s = state.current;
    const go = () => {
      if (!s.alive) return;
      if (s.running) {
        s.again = true;
        return;
      }
      const why = s.why;
      s.why = new Set();
      s.running = true;
      const generation = s.generation;
      void latest
        .current(why, () => s.alive && s.generation === generation)
        .finally(() => {
          s.running = false;
          if (s.again) {
            s.again = false;
            go();
          }
        });
    };
    go();
  }, []);

  const poke = useCallback(
    (why: Reason) => {
      const s = state.current;
      s.why.add(why);
      if (s.timer !== null) clearTimeout(s.timer);
      s.timer = setTimeout(() => {
        s.timer = null;
        run();
      }, SETTLE_MS);
    },
    [run],
  );

  // At once for what the page shows now; whatever waited to be read is read with it.
  useEffect(() => {
    const s = state.current;
    s.alive = true;
    s.generation += 1;
    if (s.timer !== null) clearTimeout(s.timer);
    s.timer = null;
    s.why.add('start');
    run();
    return () => {
      s.alive = false;
      s.generation += 1;
      if (s.timer !== null) clearTimeout(s.timer);
      s.timer = null;
    };
  }, [what, run]);

  useEffect(() => {
    const focus = () => {
      poke('focus');
    };
    window.addEventListener('focus', focus);
    const stop = onWorkspacesChanged(() => {
      poke('changed');
    });
    return () => {
      window.removeEventListener('focus', focus);
      stop();
    };
  }, [poke]);

  return poke;
}
