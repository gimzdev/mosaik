import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { type Read, type Reason, SETTLE_MS, useReads } from './reads';

const engine = vi.hoisted(() => ({
  onWorkspacesChanged: vi.fn<(callback: () => void) => () => void>(() => () => undefined),
}));
vi.mock('./engine', () => engine);

/** The engine's "workspaces changed" event. */
let changed: () => void = () => undefined;

beforeEach(() => {
  vi.useFakeTimers();
  engine.onWorkspacesChanged.mockImplementation((callback) => {
    changed = callback;
    return () => {
      changed = () => undefined;
    };
  });
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

/** A read that answers when the test says, and remembers why it was asked. */
function reads() {
  const asked: { why: Reason[]; current: () => boolean; answer: () => void }[] = [];
  const read: Read = (why, current) =>
    new Promise<void>((resolve) => {
      asked.push({ why: [...why].sort(), current, answer: resolve });
    });
  return { asked, read };
}

const focus = () => {
  act(() => {
    window.dispatchEvent(new Event('focus'));
  });
};

const later = async (ms: number) => {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
};

describe('reading what a page shows', () => {
  it('reads at once, then once for a burst of focus events and changes', async () => {
    const { asked, read } = reads();
    renderHook(() => useReads(read, 'home'));
    expect(asked.map((a) => a.why)).toEqual([['start']]);
    asked[0]?.answer();
    await later(0);
    focus();
    await later(50);
    focus();
    act(() => {
      changed();
    });
    await later(SETTLE_MS - 10);
    expect(asked).toHaveLength(1);
    await later(20);
    expect(asked.map((a) => a.why)).toEqual([['start'], ['changed', 'focus']]);
  });

  it('never reads twice at once: what is asked meanwhile is read once the read ends', async () => {
    const { asked, read } = reads();
    const { result } = renderHook(() => useReads(read, 'home'));
    focus();
    await later(SETTLE_MS + 10);
    act(() => {
      result.current('own');
    });
    await later(SETTLE_MS + 10);
    // The first read has not answered yet.
    expect(asked).toHaveLength(1);
    asked[0]?.answer();
    await later(0);
    expect(asked.map((a) => a.why)).toEqual([['start'], ['focus', 'own']]);
  });

  it('reads at once when what the page shows changes, and drops the answer about what it showed', async () => {
    const { asked, read } = reads();
    const { rerender } = renderHook(({ what }) => useReads(read, what), { initialProps: { what: 'auto' } });
    rerender({ what: 'halves' });
    const [first] = asked;
    expect(first?.current()).toBe(false);
    first?.answer();
    await later(0);
    expect(asked).toHaveLength(2);
    expect(asked[1]?.current()).toBe(true);
  });

  it('stops when the page closes', async () => {
    const { asked, read } = reads();
    const { unmount } = renderHook(() => useReads(read, 'home'));
    focus();
    unmount();
    await later(SETTLE_MS * 2);
    expect(asked).toHaveLength(1);
    expect(asked[0]?.current()).toBe(false);
  });
});
