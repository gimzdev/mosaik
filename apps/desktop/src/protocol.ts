// The engine's protocol (crates/mosaik-ipc/src/protocol.rs), as the interface sees it. The
// interface talks to the engine in process with the same commands mosaikctl sends over the pipe.
// `protocol.test.ts` checks these types against replies serialised by the Rust side.

export interface Rect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** The longest workspace or zone name, in characters (`MAX_NAME` in crates/mosaik-core). */
export const MAX_NAME = 60;

export type ZoneColor = 'lapis' | 'verdigris' | 'saffron' | 'madder' | 'slate';

/** A rectangle as fractions (0 to 1) of a monitor's work area. */
export interface RatioRect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** A zone as the zone editor shows and returns it. */
export interface ZoneEdit {
  /** null for a zone made in the editor. */
  id: string | null;
  name: string;
  color: ZoneColor;
  /** One of the monitors connected now. */
  monitor: string;
  area: RatioRect;
}

export type Theme = 'system' | 'light' | 'dark';
export type HideMethod = 'shell_cloak' | 'minimize' | 'park';
export type EdgeSide = 'left' | 'right';

/** The user's settings (crates/mosaik-core/src/settings.rs). Fields the interface does not
 * show are sent back as they came. */
export interface Settings {
  version: number;
  switcher_hotkey: string;
  snap: { enabled: boolean; invert_shift: boolean; near: number; resize_zones: boolean };
  frames: {
    show_frames: boolean;
    show_tags: boolean;
    tags_in_title_bar: boolean;
    style: { gap: number; tag_band: number; border: number };
  };
  tab_hiding: HideMethod | null;
  switching: { minimize_others: boolean; launch_missing: boolean };
  /** The quick menu's handle: on, which edge of the main monitor, and where along it (percent
   * of its height from the top). */
  edge: { enabled: boolean; side: EdgeSide; position: number };
  theme: Theme;
  start_with_windows: boolean;
  onboarding_done: boolean;
  rules: unknown;
  notices_shown: string[];
}

export type Command =
  | { cmd: 'hello'; client: string }
  | { cmd: 'state' }
  | { cmd: 'workspaces' }
  | { cmd: 'switch'; workspace: string }
  | { cmd: 'save'; workspace: string | null }
  | { cmd: 'rescue' }
  | { cmd: 'arrange'; preset?: string }
  | { cmd: 'preview_arrange'; preset?: string }
  | { cmd: 'undo_arrange' }
  | { cmd: 'show_switcher' }
  | { cmd: 'show_settings' }
  | { cmd: 'quit' }
  | { cmd: 'settings' }
  | { cmd: 'set_settings'; settings: Settings }
  | { cmd: 'check_hotkey'; hotkey: string; workspace: string | null }
  | { cmd: 'zones'; workspace: string }
  | { cmd: 'set_zones'; workspace: string; zones: ZoneEdit[]; revision: string }
  | { cmd: 'rename_workspace'; workspace: string; name: string }
  | { cmd: 'delete_workspace'; workspace: string }
  | { cmd: 'set_workspace_hotkey'; workspace: string; hotkey: string | null }
  | { cmd: 'hold_hotkeys'; hold: boolean }
  | { cmd: 'apps'; workspace: string }
  | { cmd: 'add_app'; workspace: string; window: number; zone: string | null }
  | { cmd: 'move_app'; workspace: string; app: string; zone: string }
  | { cmd: 'remove_app'; workspace: string; app: string };

export type ErrorCode = 'unsupported_version' | 'bad_request' | 'not_found' | 'busy' | 'failed' | 'unknown';

export interface WorkspaceSummary {
  id: string;
  name: string;
  windows: number;
  zones: number;
  hotkey: string | null;
  /** Its apps' names, front to back. */
  apps: string[];
}

export interface MonitorState {
  id: string;
  name: string;
  primary: boolean;
  bounds: Rect;
  work_area: Rect;
  dpi: number;
  scale_percent: number;
}

export interface HotkeyState {
  shortcut: string;
  registered: boolean;
  problem: string | null;
}

/** A zone of the main screen once arranged (`preview_arrange`). */
export interface PreviewZone {
  name: string;
  color: ZoneColor;
  rect: Rect;
}

/** A window of the main screen: where it is, and where Arrange would put it. */
export interface PreviewWindow {
  /** The app's name. */
  label: string;
  now: Rect;
  /** null: it stays where it is, or is minimised. */
  then: Rect | null;
  /** It would be minimised: every zone has a window. */
  minimized: boolean;
}

/** An app of a workspace. */
export interface WorkspaceApp {
  id: string;
  label: string;
  /** The zone it goes in (a tab: its group's). */
  zone: string | null;
  /** It is a tab of a group. */
  tab: boolean;
}

/** A zone an app can go in. */
export interface AppZone {
  id: string;
  name: string;
  color: ZoneColor;
}

/** A window on screen that is not part of the workspace. */
export interface OpenWindow {
  window: number;
  label: string;
  title: string;
}

/** A workspace's apps, its zones, and the windows on screen it could take (`apps`). */
export interface WorkspaceApps {
  workspace: string;
  name: string;
  apps: WorkspaceApp[];
  zones: AppZone[];
  windows: OpenWindow[];
}

/** What a screen's windows say the user is doing (`task::Task`). */
export type Task = 'coding' | 'design' | 'writing' | 'browsing' | 'talking' | 'playing' | 'general';

/** What a layout made for a task is for, in a few words; none when nothing says. */
export const TASK_WORDS: Record<Task, string | null> = {
  coding: 'coding',
  design: 'design',
  writing: 'writing',
  browsing: 'browsing',
  talking: 'chat and mail',
  playing: 'games',
  general: null,
};

const isTask = (v: unknown): v is Task => typeof v === 'string' && Object.hasOwn(TASK_WORDS, v);

/** What Arrange (with a layout, or without) would do on the main screen, in screen pixels. */
export interface ArrangePreview {
  work_area: Rect;
  zones: PreviewZone[];
  windows: PreviewWindow[];
  /** Windows on the other screens, which Arrange puts in zones there. */
  elsewhere: number;
  /** What the main screen's windows say the user is doing, which Arrange lays it out for
   * (none for a layout picked). */
  task?: Task | null;
}

export type Reply =
  | { reply: 'hello'; app_version: string; protocol: number }
  | ({ reply: 'preview' } & ArrangePreview)
  | ({ reply: 'apps' } & WorkspaceApps)
  | { reply: 'workspaces'; workspaces: WorkspaceSummary[]; active: string | null }
  | { reply: 'done'; message: string }
  | { reply: 'error'; code: ErrorCode; message: string; hint: string | null }
  | {
      reply: 'state';
      app_version: string;
      protocol: number;
      safe_mode: boolean;
      watchdog: boolean;
      monitors: MonitorState[];
      hotkey: HotkeyState;
    }
  /** The settings, and the switcher shortcut as registered (another one when the chosen one
   * is taken by another app). */
  | { reply: 'settings'; settings: Settings; hotkey: HotkeyState }
  | { reply: 'hotkey'; hotkey: string; problem: string | null; warnings: string[] }
  | {
      reply: 'zones';
      workspace: string;
      name: string;
      monitors: MonitorState[];
      zones: ZoneEdit[];
      /** Sent back with `set_zones`, which is refused if the zones or monitors changed since. */
      revision: string;
    };

/** Something the user should be told once. */
export interface Notice {
  title: string;
  body: string;
}

/** The reply's message when it is an error, for showing to the user. */
export function errorMessage(reply: Reply): string | null {
  if (reply.reply !== 'error') return null;
  return reply.hint ? `${reply.message} ${reply.hint}` : reply.message;
}

function isObject(x: unknown): x is Record<string, unknown> {
  return typeof x === 'object' && x !== null;
}

const isString = (x: unknown): x is string => typeof x === 'string';
const isNumber = (x: unknown): x is number => typeof x === 'number';
const isBoolean = (x: unknown): x is boolean => typeof x === 'boolean';
const isNullableString = (x: unknown): x is string | null => x === null || isString(x);

const COLORS = new Set<string>(['lapis', 'verdigris', 'saffron', 'madder', 'slate']);
const THEMES = new Set<string>(['system', 'light', 'dark']);
const HIDE_METHODS = new Set<string>(['shell_cloak', 'minimize', 'park']);
const EDGE_SIDES = new Set<string>(['left', 'right']);

function isRect(x: unknown): x is Rect {
  return isObject(x) && isNumber(x.left) && isNumber(x.top) && isNumber(x.right) && isNumber(x.bottom);
}

function isMonitor(x: unknown): x is MonitorState {
  return (
    isObject(x) &&
    isString(x.id) &&
    isString(x.name) &&
    isBoolean(x.primary) &&
    isRect(x.bounds) &&
    isRect(x.work_area) &&
    isNumber(x.dpi) &&
    isNumber(x.scale_percent)
  );
}

function isZone(x: unknown): x is ZoneEdit {
  return (
    isObject(x) &&
    isNullableString(x.id) &&
    isString(x.name) &&
    isString(x.color) &&
    COLORS.has(x.color) &&
    isString(x.monitor) &&
    isRect(x.area)
  );
}

export function isSettings(x: unknown): x is Settings {
  if (!isObject(x)) return false;
  const { snap, frames, switching, edge } = x;
  return (
    isNumber(x.version) &&
    isString(x.switcher_hotkey) &&
    isObject(snap) &&
    isBoolean(snap.enabled) &&
    isBoolean(snap.invert_shift) &&
    isBoolean(snap.resize_zones) &&
    isObject(edge) &&
    isBoolean(edge.enabled) &&
    isString(edge.side) &&
    EDGE_SIDES.has(edge.side) &&
    isNumber(edge.position) &&
    isObject(frames) &&
    isBoolean(frames.show_frames) &&
    isBoolean(frames.show_tags) &&
    isBoolean(frames.tags_in_title_bar) &&
    (x.tab_hiding === null || (isString(x.tab_hiding) && HIDE_METHODS.has(x.tab_hiding))) &&
    isObject(switching) &&
    isBoolean(switching.minimize_others) &&
    isBoolean(switching.launch_missing) &&
    isString(x.theme) &&
    THEMES.has(x.theme) &&
    isBoolean(x.start_with_windows) &&
    isBoolean(x.onboarding_done) &&
    Array.isArray(x.notices_shown)
  );
}

function isPreviewZone(x: unknown): x is PreviewZone {
  return isObject(x) && isString(x.name) && isString(x.color) && COLORS.has(x.color) && isRect(x.rect);
}

function isPreviewWindow(x: unknown): x is PreviewWindow {
  return (
    isObject(x) && isString(x.label) && isRect(x.now) && (x.then === null || isRect(x.then)) && isBoolean(x.minimized)
  );
}

function isWorkspaceApp(x: unknown): x is WorkspaceApp {
  return isObject(x) && isString(x.id) && isString(x.label) && isNullableString(x.zone) && isBoolean(x.tab);
}

function isAppZone(x: unknown): x is AppZone {
  return isObject(x) && isString(x.id) && isString(x.name) && isString(x.color) && COLORS.has(x.color);
}

function isOpenWindow(x: unknown): x is OpenWindow {
  return isObject(x) && isNumber(x.window) && isString(x.label) && isString(x.title);
}

function isHotkeyState(x: unknown): x is HotkeyState {
  return isObject(x) && isString(x.shortcut) && isBoolean(x.registered) && isNullableString(x.problem);
}

function isWorkspace(x: unknown): x is WorkspaceSummary {
  return (
    isObject(x) &&
    isString(x.id) &&
    isString(x.name) &&
    isNumber(x.windows) &&
    isNumber(x.zones) &&
    isNullableString(x.hotkey) &&
    Array.isArray(x.apps) &&
    x.apps.every(isString)
  );
}

/** Checks a reply's shape at run time, so a mismatch with the engine shows as an error. */
export function isReply(x: unknown): x is Reply {
  if (!isObject(x)) return false;
  switch (x.reply) {
    case 'hello':
      return isString(x.app_version) && isNumber(x.protocol);
    case 'workspaces':
      return Array.isArray(x.workspaces) && x.workspaces.every(isWorkspace) && isNullableString(x.active);
    case 'done':
      return isString(x.message);
    case 'error':
      return isString(x.code) && isString(x.message) && isNullableString(x.hint);
    case 'state':
      return isString(x.app_version) && isBoolean(x.safe_mode) && Array.isArray(x.monitors) && isHotkeyState(x.hotkey);
    case 'settings':
      return isSettings(x.settings) && isHotkeyState(x.hotkey);
    case 'hotkey':
      return (
        isString(x.hotkey) && isNullableString(x.problem) && Array.isArray(x.warnings) && x.warnings.every(isString)
      );
    case 'zones':
      return (
        isString(x.workspace) &&
        isString(x.name) &&
        Array.isArray(x.monitors) &&
        x.monitors.every(isMonitor) &&
        Array.isArray(x.zones) &&
        x.zones.every(isZone) &&
        isString(x.revision)
      );
    case 'preview':
      return (
        isRect(x.work_area) &&
        Array.isArray(x.zones) &&
        x.zones.every(isPreviewZone) &&
        Array.isArray(x.windows) &&
        x.windows.every(isPreviewWindow) &&
        isNumber(x.elsewhere) &&
        (x.task === undefined || x.task === null || isTask(x.task))
      );
    case 'apps':
      return (
        isString(x.workspace) &&
        isString(x.name) &&
        Array.isArray(x.apps) &&
        x.apps.every(isWorkspaceApp) &&
        Array.isArray(x.zones) &&
        x.zones.every(isAppZone) &&
        Array.isArray(x.windows) &&
        x.windows.every(isOpenWindow)
      );
    default:
      return false;
  }
}
