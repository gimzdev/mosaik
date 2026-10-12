// Recording a global shortcut: press it, Mosaïk checks it (AltGr on the installed keyboard
// layouts, Windows' own shortcuts, other apps), then save it.

import { type KeyboardEvent, useId, useRef, useState } from 'react';

import { request } from './engine';
import { errorMessage } from './protocol';
import { Button, Keys, Status } from './ui';

const NAMED: Record<string, string> = {
  Space: 'Space',
  Enter: 'Enter',
  Tab: 'Tab',
  Backspace: 'Backspace',
  Pause: 'Pause',
  PageUp: 'PageUp',
  PageDown: 'PageDown',
  End: 'End',
  Home: 'Home',
  ArrowLeft: 'Left',
  ArrowUp: 'Up',
  ArrowRight: 'Right',
  ArrowDown: 'Down',
  PrintScreen: 'PrintScreen',
  Insert: 'Insert',
  Delete: 'Delete',
  NumpadMultiply: 'NumpadMultiply',
  NumpadAdd: 'NumpadAdd',
  NumpadSubtract: 'NumpadSubtract',
  NumpadDecimal: 'NumpadDecimal',
  NumpadDivide: 'NumpadDivide',
  Semicolon: 'Semicolon',
  Equal: 'Equals',
  Comma: 'Comma',
  Minus: 'Minus',
  Period: 'Period',
  Slash: 'Slash',
  Backquote: 'Backquote',
  BracketLeft: 'LeftBracket',
  Backslash: 'Backslash',
  BracketRight: 'RightBracket',
  Quote: 'Quote',
  IntlBackslash: 'IntlBackslash',
};

const MODIFIER_KEYS = new Set(['Shift', 'Control', 'Alt', 'Meta', 'OS', 'AltGraph']);

/** The key of a key press, as Mosaïk writes it, or null for a key no shortcut may use. Letters
 * follow the keyboard's own labels (an AZERTY "A" is A), like Windows' virtual keys. */
export function keyName(key: string, code: string): string | null {
  if (/^[a-z]$/i.test(key)) return key.toUpperCase();
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  if (/^Numpad[0-9]$/.test(code)) return code;
  return NAMED[code] ?? null;
}

export type Recorded = { shortcut: string } | { problem: string } | null;

/** What a key press records: a shortcut, a reason it cannot be one, or nothing yet (a modifier
 * alone, still being held). */
export function record(e: {
  key: string;
  code: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  altGraph: boolean;
}): Recorded {
  if (MODIFIER_KEYS.has(e.key)) return null;
  if (e.altGraph) {
    return { problem: 'AltGr types characters on your keyboard. Use Win, Ctrl or Alt instead.' };
  }
  const key = keyName(e.key, e.code);
  if (!key) return { problem: 'That key cannot be part of a shortcut.' };
  const mods = [e.metaKey && 'Win', e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift'].filter(
    (m): m is string => typeof m === 'string',
  );
  return { shortcut: [...mods, key].join('+') };
}

export function ShortcutRecorder({
  label,
  value,
  workspace = null,
  onSave,
  onRemove,
}: {
  label: string;
  /** The shortcut now, or null when there is none. */
  value: string | null;
  /** The workspace the shortcut is for; null for the switcher. */
  workspace?: string | null;
  /** Saves the new shortcut; resolves to an error message, or null when saved. */
  onSave: (shortcut: string) => Promise<string | null>;
  onRemove?: () => Promise<string | null>;
}) {
  const [recording, setRecording] = useState(false);
  const [candidate, setCandidate] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [notes, setNotes] = useState<string[]>([]);
  const [status, setStatus] = useState<string | null>(null);
  const box = useRef<HTMLButtonElement>(null);
  const hint = useId();

  const start = () => {
    setRecording(true);
    setCandidate(null);
    setProblem(null);
    setNotes([]);
    setStatus(null);
    requestAnimationFrame(() => box.current?.focus());
  };
  const stop = () => {
    setRecording(false);
    setCandidate(null);
    setProblem(null);
    setNotes([]);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape' && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
      stop();
      return;
    }
    const r = record({
      key: e.key,
      code: e.code,
      metaKey: e.metaKey,
      ctrlKey: e.ctrlKey,
      altKey: e.altKey,
      shiftKey: e.shiftKey,
      altGraph: e.getModifierState('AltGraph'),
    });
    if (!r) return;
    if ('problem' in r) {
      setCandidate(null);
      setProblem(r.problem);
      return;
    }
    setCandidate(r.shortcut);
    setProblem(null);
    void request({ cmd: 'check_hotkey', hotkey: r.shortcut, workspace }).then((reply) => {
      if (reply.reply === 'hotkey') {
        setCandidate(reply.hotkey);
        setProblem(reply.problem);
        setNotes(reply.warnings);
      } else {
        setProblem(errorMessage(reply) ?? 'Mosaïk could not check this shortcut.');
      }
    });
  };

  const save = async () => {
    if (!candidate) return;
    const error = await onSave(candidate);
    if (error) {
      setProblem(error);
      return;
    }
    stop();
    setStatus(`${candidate} saved.`);
  };

  return (
    <div className="py-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p className="font-semibold">{label}</p>
          <div className="mt-1">
            {value ? <Keys shortcut={value} /> : <span className="text-[13px] text-[var(--muted)]">No shortcut</span>}
          </div>
        </div>
        {recording ? null : (
          <div className="flex gap-2">
            <Button onClick={start} label={`Change the shortcut: ${label}`}>
              {value ? 'Change' : 'Add a shortcut'}
            </Button>
            {value && onRemove ? (
              <Button
                tone="quiet"
                label={`Remove the shortcut: ${label}`}
                onClick={() => {
                  void onRemove().then((error) => {
                    setStatus(error ?? 'Shortcut removed.');
                  });
                }}
              >
                Remove
              </Button>
            ) : null}
          </div>
        )}
      </div>
      {recording ? (
        <div className="mt-3 flex flex-col gap-2">
          <button
            ref={box}
            type="button"
            aria-describedby={hint}
            aria-label={candidate ? `New shortcut: ${candidate}` : 'Press the new shortcut'}
            onKeyDown={onKeyDown}
            className="flex min-h-11 items-center justify-center rounded-lg border-2 border-dashed border-[var(--accent)] bg-[var(--bg)] px-3 text-[13px]"
          >
            {candidate ? <Keys shortcut={candidate} /> : 'Press the new shortcut…'}
          </button>
          <p id={hint} className="text-[12px] text-[var(--muted)]">
            Hold Win, Ctrl or Alt and press a key. Esc cancels.
          </p>
          <Status text={problem} error />
          {notes.map((n) => (
            <p key={n} className="text-[12px] text-[var(--muted)]">
              {n}
            </p>
          ))}
          <div className="flex gap-2">
            <Button
              tone="primary"
              disabled={!candidate || problem !== null}
              onClick={() => {
                void save();
              }}
            >
              Save shortcut
            </Button>
            <Button onClick={stop}>Cancel</Button>
          </div>
        </div>
      ) : (
        <Status text={status} />
      )}
    </div>
  );
}
