import type { IntegrationView, UsageWindow as ApiUsageWindow } from '../../companion/api.ts';
import type { ConfigField, DataStream, IntegrationSummary, JsonValue, PresetTile } from '../../companion/api.ts';
import { plural } from '../editor/messages.ts';
import { parseLayout, type Action } from '../editor/validate.ts';

interface FieldCopy {
  label: string;
  placeholder?: string;
  unit?: string;
}

interface IntegrationCopy {
  line: string;
  fields: Partial<Record<string, FieldCopy>>;
}

const COPY: Partial<Record<string, IntegrationCopy>> = {
  cliproxyapi: {
    line: "Shows how much of each AI account's limit is used, read from your CLIProxyAPI hub.",
    fields: {
      enabled: { label: 'Turn on CLIProxyAPI' },
      url: { label: 'Hub address', placeholder: 'https://hub.example.com' },
      interval_s: { label: 'Check every', unit: 's' },
      management_key: { label: 'Management key' },
    },
  },
};

const sentence = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

const humanize = (name: string) => sentence(name.replaceAll('_', ' ').trim());

/** The one line under an Integration's name. */
export const lineOf = (integration: string) => COPY[integration]?.line ?? '';

export type FieldKind = 'boolean' | 'integer' | 'number' | 'string';

export interface FormField {
  name: string;
  kind: FieldKind;
  label: string;
  placeholder?: string;
  unit?: string;
}

const KINDS: readonly string[] = ['boolean', 'integer', 'number', 'string'];
const isFieldKind = (kind: string): kind is FieldKind => KINDS.includes(kind);

/** The form an Integration's schema builds: its settings, then its write-only secret fields. */
export function formOf(integration: string, schema: readonly ConfigField[]): { fields: FormField[]; secrets: FormField[] } {
  const field = (f: ConfigField): FormField => {
    const copy = COPY[integration]?.fields[f.name];
    return { name: f.name, kind: isFieldKind(f.kind) ? f.kind : 'string', label: copy?.label ?? humanize(f.name), placeholder: copy?.placeholder, unit: copy?.unit };
  };
  return { fields: schema.filter((f) => !f.secret).map(field), secrets: schema.filter((f) => f.secret).map(field) };
}

/** What the form's controls hold: a switch holds a boolean, every other field the text typed in it. */
export type FormValues = Partial<Record<string, string | boolean>>;

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === 'object' && value !== null && !Array.isArray(value);

/** The form's starting values, from the config the Companion holds. */
export function valuesOf(fields: readonly FormField[], config: JsonValue): FormValues {
  const source = isRecord(config) ? config : {};
  const values: FormValues = {};
  for (const field of fields) {
    const value = source[field.name];
    if (field.kind === 'boolean') values[field.name] = value === true;
    else if (typeof value === 'number' || typeof value === 'string') values[field.name] = String(value);
    else values[field.name] = '';
  }
  return values;
}

const text = (values: FormValues, name: string) => {
  const value = values[name];
  return typeof value === 'string' ? value.trim() : '';
};

/** Each field the schema's kinds refuse, with the sentence to show under it. */
export function validate(fields: readonly FormField[], values: FormValues): Partial<Record<string, string>> {
  const errors: Partial<Record<string, string>> = {};
  for (const field of fields) {
    const value = text(values, field.name);
    if (field.kind === 'integer' && !/^\d+$/.test(value)) errors[field.name] = 'Enter a whole number.';
    if (field.kind === 'number' && (value === '' || !Number.isFinite(Number(value)))) errors[field.name] = 'Enter a number.';
  }
  return errors;
}

/** The config to send. Only the schema's settings go in it, never a secret field. */
export function serialize(fields: readonly FormField[], values: FormValues): Record<string, JsonValue> {
  const config: Record<string, JsonValue> = {};
  for (const field of fields) {
    if (field.kind === 'boolean') config[field.name] = values[field.name] === true;
    else if (field.kind === 'integer' || field.kind === 'number') config[field.name] = Number(text(values, field.name));
    else {
      const value = values[field.name];
      config[field.name] = typeof value === 'string' ? value : '';
    }
  }
  return config;
}

export const sameValues = (fields: readonly FormField[], a: FormValues, b: FormValues) => fields.every((f) => a[f.name] === b[f.name]);

/**
 * How a key would travel to an address, as the Companion decides it: https always, plain http only to this PC or a
 * private network address typed as an IP, and never to anything else.
 */
export type Transport = 'secure' | 'local' | 'unencrypted' | 'refused' | 'invalid';

function privateHost(host: string): boolean {
  const v4 = /^(\d+)\.(\d+)\.(\d+)\.(\d+)$/.exec(host);
  if (v4) {
    const [a, b] = [Number(v4[1]), Number(v4[2])];
    return a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168);
  }
  return /^\[f[cd][0-9a-f]{0,2}:/i.test(host);
}

const loopbackHost = (host: string) => host === 'localhost' || host === '[::1]' || /^127\.\d+\.\d+\.\d+$/.test(host);

export function transportOf(address: string): Transport {
  let url: URL;
  try {
    url = new URL(address.trim());
  } catch {
    return 'invalid';
  }
  if (url.protocol === 'https:') return 'secure';
  if (url.protocol !== 'http:') return 'invalid';
  if (loopbackHost(url.hostname)) return 'local';
  return privateHost(url.hostname) ? 'unencrypted' : 'refused';
}

/** The warning for an address a key travels to, or null when it travels safely. */
export function transportWarning(address: string): string | null {
  const transport = transportOf(address);
  switch (transport) {
    case 'unencrypted':
      return "This address isn't on this PC and uses http://, so the key is sent unencrypted. Use https://.";
    case 'refused':
      return "This address uses http:// and isn't on your network, so the key isn't sent. Use https://.";
    case 'secure':
    case 'local':
    case 'invalid':
      return null;
    default: {
      const exhaustive: never = transport;
      return exhaustive;
    }
  }
}

export interface Status {
  words: string;
  /** Green only while connected. */
  live: boolean;
  /** No credentials yet, so the row offers Set up instead of a light. */
  needsSetup: boolean;
  /** The Companion's raw reason, for Details. */
  detail: string | null;
}

export function statusOf(summary: Pick<IntegrationSummary, 'enabled' | 'status'>): Status {
  const { state, message } = summary.status;
  if (state === 'not_configured') return { words: 'Not set up', live: false, needsSetup: true, detail: null };
  if (!summary.enabled) return { words: 'Off', live: false, needsSetup: false, detail: null };
  if (state === 'ok') return { words: 'Connected', live: true, needsSetup: false, detail: null };
  if (state === 'error') return { words: "Can't connect", live: false, needsSetup: false, detail: message };
  return { words: 'Checkingâ€¦', live: false, needsSetup: false, detail: message };
}

/** Whether an Integration is set up and on, so the Add Tile panel offers its Preset tiles. */
export const offersPresets = (summary: IntegrationSummary) => summary.enabled && summary.status.state !== 'not_configured' && summary.presets.length > 0;

export function ago(unixMs: number | null, now: number): string {
  if (unixMs === null) return 'Never';
  const seconds = Math.max(0, Math.round((now - unixMs) / 1000));
  if (seconds < 90) return `${seconds} s ago`;
  const minutes = Math.round(seconds / 60);
  return minutes < 90 ? `${minutes} min ago` : `${Math.round(minutes / 60)} hr ago`;
}

export interface UsageWindow {
  pct: number;
  /** `now`, a time such as `2h 10m`, or null when the hub doesn't say. */
  reset_in: string | null;
}

export interface UsageAccount {
  key: string;
  provider: string;
  status: string;
  plan: string | null;
  success: number;
  failed: number;
  recent_10m: number;
  five_hour: UsageWindow | null;
  seven_day: UsageWindow | null;
  usage_error: string | null;
}

export interface Usage {
  polled_ms: number | null;
  fails: number;
  stale: boolean;
  accounts: UsageAccount[];
}

export function parseUsage(view: IntegrationView): Usage | null {
  const { detail } = view;
  if (!detail) return null;
  const window = (value: ApiUsageWindow | null): UsageWindow | null => {
    if (!value) return null;
    const minutes = value.resets_at_ms === null ? null : Math.max(0, Math.ceil((value.resets_at_ms - detail.polled_ms) / 60_000));
    const reset_in = minutes === null ? null : minutes === 0 ? 'now' : minutes >= 1440 ? `${Math.floor(minutes / 1440)}d ${Math.floor(minutes % 1440 / 60)}h` : minutes >= 60 ? `${Math.floor(minutes / 60)}h ${minutes % 60}m` : `${minutes}m`;
    return { pct: value.pct, reset_in };
  };
  return {
    polled_ms: detail.polled_ms,
    fails: 0,
    stale: view.summary.status.state === 'error',
    accounts: detail.accounts.map(account => ({
      ...account,
      key: `${account.provider}:${account.account}`,
      five_hour: window(account.five_hour),
      seven_day: window(account.seven_day),
      usage_error: account.error,
    })),
  };
}

/** The status line above the accounts, from the last checks. */
export function usageLine(usage: Usage, now: number): string {
  if (usage.polled_ms === null && usage.fails === 0) return 'Checkingâ€¦';
  if (usage.stale) return `Can't connect. The hub missed ${plural(usage.fails, 'check')}, so its Tiles are greyed out.`;
  if (usage.fails > 0) return `The hub missed the last ${plural(usage.fails, 'check')}. Retrying.`;
  return `Connected. ${plural(usage.accounts.length, 'account')}, checked ${ago(usage.polled_ms, now).toLowerCase()}.`;
}

const BADGES: Partial<Record<string, string>> = { active: 'Active', disabled: 'Disabled', unavailable: 'Unavailable', error: 'Error' };

export const badgeOf = (status: string) => BADGES[status.toLowerCase()] ?? sentence(status);

export function resetLine(window: UsageWindow): string {
  if (window.reset_in === 'now') return 'Resets now';
  return window.reset_in ? `Resets in ${window.reset_in}` : 'Reset time unknown';
}

export const countsLine = (account: UsageAccount) => `${account.success} OK Â· ${account.failed} failed Â· ${account.recent_10m} in 10 min`;

const TILE_LABEL_MAX = 16;

/** A Preset tile's name as a Tile label: sentence case and at most 16 characters. */
export function presetLabel(name: string): string {
  const label = [...sentence(name.trim())].slice(0, TILE_LABEL_MAX).join('').trim();
  return label || 'Tile';
}

/** The Preset's action as a Layout Tile action, or null when the Layout rules refuse it. */
export function parseAction(value: JsonValue): Action | null {
  const probe = {
    schema: 1,
    name: 'Preset',
    orientation: 'landscape',
    pages: [{ kind: 'grid', id: 'p', title: 'P', rows: 1, cols: 1, tiles: [{ id: 't', label: 'T', row: 0, col: 0, rowSpan: 1, colSpan: 1, icon: null, action: value }] }],
  };
  const parsed = parseLayout(JSON.stringify(probe));
  if (!parsed.ok) return null;
  const [page] = parsed.layout.pages;
  return page.kind === 'grid' ? page.tiles[0].action : null;
}

/** How a Preset tile is drawn: a bar for a share, a graph for a count, or its value as text. */
export type PresetLook = 'bar' | 'graph' | 'text';

export function presetLook(preset: PresetTile, streams: readonly DataStream[]): PresetLook {
  const action = parseAction(preset.action);
  const stream = action?.type === 'widget' ? streams.find((s) => s.id === action.field) : undefined;
  if (stream?.unit === '%') return 'bar';
  return stream?.kind === 'numeric' ? 'graph' : 'text';
}
