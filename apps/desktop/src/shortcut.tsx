// Recording a global shortcut: press it, Mosaïk checks it (AltGr on the installed keyboard
// layouts, Windows' own shortcuts, other apps), then save it.

import { type KeyboardEvent, useEffect, useId, useRef, useState } from 'react';

import { request } from './engine';
import { errorMessage } from './protocol';
import { Button, Keys, Status } from './ui';

/** Keys named by what they do, from `KeyboardEvent.code`. */
const NAMED: Record<string, string> = {
  Space: 'Space',
  Enter: 'Enter',
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
};

/** Punctuation keys by their Windows virtual-key code, which is what Mosaïk registers: the key
 * that types "ü" on a German keyboard is VK_OEM_1 there, wherever a US keyboard has it. */
const PUNCTUATION: Record<number, string> = {
  0xba: 'Semicolon',
  0xbb: 'Equals',
  0xbc: 'Comma',
  0xbd: 'Minus',
  0xbe: 'Period',
  0xbf: 'Slash',
  0xc0: 'Backquote',
  0xdb: 'LeftBracket',
  0xdc: 'Backslash',
  0xdd: 'RightBracket',
  0xde: 'Quote',
  0xe2: 'IntlBackslash',
};

const MODIFIER_KEYS = new Set(['Shift', 'Control', 'Alt', 'Meta', 'OS', 'AltGraph']);

/** The key of a key press, as Mosaïk writes it, or null for a key no shortcut may use. Letters
 * follow the keyboard's own labels (an AZERTY "A" is A), like Windows' virtual keys; punctuation
 * follows the virtual key (`keyCode`, which WebView2 takes from Windows). */
export function keyName(key: string, code: string, keyCode: number): string | null {
  if (/^[a-z]$/i.test(key)) return key.toUpperCase();
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  if (/^Numpad[0-9]$/.test(code)) return code;
  return PUNCTUATION[keyCode] ?? NAMED[code] ?? null;
}

/** Keys that type something: with AltGr they type characters, so they are never shortcuts. */
function types(name: string): boolean {
  return /^([A-Z0-9]|Space)$/.test(name) || Object.values(PUNCTUATION).includes(name);
}

export type Recorded = { shortcut: string } | { problem: string } | null;

/** What a key press records: a shortcut, a reason it cannot be one, or nothing yet (a modifier
 * alone, still being held). */
export function record(e: {
  key: string;
  code: string;
  keyCode: number;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  altGraph: boolean;
}): Recorded {
  if (MODIFIER_KEYS.has(e.key)) return null;
  const key = keyName(e.key, e.code, e.keyCode);
  if (!key) return { problem: 'That key cannot be part of a shortcut.' };
  // AltGr is Ctrl+Alt to Windows: fine with F5, never with a key that types.
  if (e.altGraph && types(key)) {
    return { problem: 'AltGr with this key types a character on your keyboard. Use Win, Ctrl or Alt instead.' };
  }
  const ctrl = e.ctrlKey || e.altGraph;
  const alt = e.altKey || e.altGraph;
  const mods = [e.metaKey && 'Win', ctrl && 'Ctrl', alt && 'Alt', e.shiftKey && 'Shift'].filter(
    (m): m is string => typeof m === 'string',
  );
  return { shortcut: [...mods, key].join('+') };
}

/** Mosaïk's own shortcuts let go while one is recorded, so pressing one reaches the recorder
 * instead of opening the switcher or a workspace; they come back when recording ends. */
function holdShortcuts(hold: boolean) {
  void request({ cmd: 'hold_hotkeys', hold });
}

export function ShortcutRecorder({
  label,
  value,
  workspace = null,
  autoStart = false,
  onSave,
  onRemove,
  onClose,
}: {
  label: string;
  /** The shortcut now, or null when there is none. */
  value: string | null;
  /** The workspace the shortcut is for; null for the switcher. */
  workspace?: string | null;
  /** Start recording at once (the shortcut was asked for from a menu). */
  autoStart?: boolean;
  /** Saves the new shortcut; resolves to an error message, or null when saved. */
  onSave: (shortcut: string) => Promise<string | null>;
  onRemove?: () => Promise<string | null>;
  /** Recording ended without a change (Cancel, Esc). */
  onClose?: () => void;
}) {
  const [recording, setRecording] = useState(autoStart);
  const [candidate, setCandidate] = useState<string | null>(null);
  /** The candidate Mosaïk has checked; Save waits for it. */
  const [checked, setChecked] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [notes, setNotes] = useState<string[]>([]);
  const [pending, setPending] = useState(false);
  const [removeProblem, setRemoveProblem] = useState<string | null>(null);
  const box = useRef<HTMLButtonElement>(null);
  /** The last shortcut pressed: a check that comes back for an earlier one is dropped. */
  const latest = useRef<string | null>(null);
  const changeButton = useRef<HTMLDivElement>(null);
  const hint = useId();
  const visible = value ? 'Change' : 'Add a shortcut';

  const start = () => {
    setRecording(true);
    setCandidate(null);
    setChecked(null);
    setProblem(null);
    setNotes([]);
    setRemoveProblem(null);
  };

  const stop = (closed: boolean) => {
    setRecording(false);
    setCandidate(null);
    setChecked(null);
    setProblem(null);
    setNotes([]);
    if (closed && onClose) onClose();
    else requestAnimationFrame(() => changeButton.current?.querySelector('button')?.focus());
  };

  // Held while recording; back when recording ends, however it ends (this page closing too).
  // The box takes the focus, so the next keys pressed are the shortcut.
  useEffect(() => {
    if (!recording) return;
    box.current?.focus();
    holdShortcuts(true);
    return () => {
      holdShortcuts(false);
    };
  }, [recording]);

  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    // Tab moves on (to Save), so the keyboard alone can save what was recorded.
    if (e.key === 'Tab' && !e.metaKey && !e.ctrlKey && !e.altKey) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape' && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
      stop(true);
      return;
    }
    const r = record({
      key: e.key,
      code: e.code,
      // The Windows virtual-key code: the only name of a punctuation key that matches what
      // Windows registers, whatever the keyboard layout.
      // eslint-disable-next-line @typescript-eslint/no-deprecated
      keyCode: e.keyCode,
      metaKey: e.metaKey,
      ctrlKey: e.ctrlKey,
      altKey: e.altKey,
      shiftKey: e.shiftKey,
      altGraph: e.getModifierState('AltGraph'),
    });
    if (!r) return;
    if ('problem' in r) {
      latest.current = null;
      setCandidate(null);
      setChecked(null);
      setNotes([]);
      setProblem(r.problem);
      return;
    }
    const shortcut = r.shortcut;
    latest.current = shortcut;
    setCandidate(shortcut);
    setChecked(null);
    setProblem(null);
    setNotes([]);
    void request({ cmd: 'check_hotkey', hotkey: shortcut, workspace }).then((reply) => {
      if (latest.current !== shortcut) return;
      if (reply.reply === 'hotkey') {
        latest.current = reply.hotkey;
        setCandidate(reply.hotkey);
        setChecked(reply.problem ? null : reply.hotkey);
        setProblem(reply.problem);
        setNotes(reply.warnings);
      } else {
        setProblem(errorMessage(reply) ?? 'Mosaïk could not check this shortcut.');
      }
    });
  };

  const save = async () => {
    if (!checked) return;
    setPending(true);
    const error = await onSave(checked);
    setPending(false);
    if (error) {
      setProblem(error);
      return;
    }
    stop(false);
  };

  return (
    <div className="py-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p className="font-semibold">{label}</p>
          <div className="mt-1">
            {value ? <Keys shortcut={value} /> : <span className="text-small text-[var(--muted)]">No shortcut</span>}
          </div>
        </div>
        {recording ? null : (
          <div ref={changeButton} className="flex gap-2">
            <Button onClick={start} label={`${visible}: ${label}`}>
              {visible}
            </Button>
            {value && onRemove ? (
              <Button
                tone="quiet"
                label={`Remove: ${label}`}
                onClick={() => {
                  void onRemove().then(setRemoveProblem);
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
            className="flex min-h-11 items-center justify-center rounded-lg border-2 border-dashed border-[var(--accent)] bg-[var(--bg)] px-3 text-small"
          >
            {candidate ? <Keys shortcut={candidate} /> : 'Press the new shortcut…'}
          </button>
          <p id={hint} className="text-caption text-[var(--muted)]">
            Hold Win, Ctrl or Alt and press a key. Esc cancels; Tab moves on to Save.
          </p>
          {/* One slot for what Mosaïk says about the shortcut, so the buttons do not move
              further down when there is nothing to say. */}
          <div>
            <Status text={problem} error />
            <div role="status">
              {notes.map((n) => (
                <p key={n} className="text-caption text-[var(--muted)]">
                  {n}
                </p>
              ))}
            </div>
          </div>
          <div className="flex gap-2">
            <Button
              tone="primary"
              pending={pending}
              disabled={!checked || checked !== candidate}
              onClick={() => {
                void save();
              }}
            >
              Save shortcut
            </Button>
            <Button
              onClick={() => {
                stop(true);
              }}
            >
              Cancel
            </Button>
          </div>
        </div>
      ) : (
        <Status text={removeProblem} error />
      )}
    </div>
  );
}
