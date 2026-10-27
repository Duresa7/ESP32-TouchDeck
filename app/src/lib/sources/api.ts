import { invoke } from '@tauri-apps/api/core';

export interface Reading {
  source: string;
  account: string;
  field: string;
  value: string;
  bar: number;
  updated: number;
  stale: boolean;
}

export interface ClaudeInfo {
  dir: string;
  session: string | null;
  updated: number | null;
  stale: boolean;
  error: string | null;
  readings: Reading[];
}

export interface UsageWindow {
  pct: number;
  resets_at: number | null;
  reset_in: string | null;
}

export const PROXY_FIELDS = ['five_hour_pct', 'seven_day_pct', 'five_hour_reset', 'seven_day_reset', 'plan', 'status', 'success', 'failed', 'recent_10m'];

export interface Account {
  key: string;
  provider: string;
  email: string;
  status: string;
  success: number;
  failed: number;
  recent_10m: number;
  plan: string | null;
  five_hour: UsageWindow | null;
  seven_day: UsageWindow | null;
  usage_error: string | null;
  insecure_http: boolean;
}

export interface ProxyInfo {
  url: string;
  insecure_http: boolean;
  key_hint: string | null;
  ok: boolean | null;
  fails: number;
  stale: boolean;
  error: string | null;
  polled: number | null;
  accounts: Account[];
}

export interface LayoutInfo {
  name: string | null;
  widgets: number;
  dashboards: number;
  error: string | null;
}

export interface Status {
  claude: ClaudeInfo;
  proxy: ProxyInfo;
  layout: LayoutInfo;
  deck: boolean;
  sent: number;
  send_error: string | null;
}

export interface Config {
  claude_status: boolean;
  claude_dir: string | null;
  cliproxy: boolean;
  cliproxy_url: string;
  cliproxy_interval_s: number;
  layout_path: string | null;
  claude_settings: string | null;
}

export interface SourcesView {
  config: Config;
  key_hint: string | null;
  claude_dir: string;
  settings_path: string;
  config_file: string;
  shim: { command: string; found: boolean };
  mock: boolean;
}

export interface DiffLine {
  kind: 'ctx' | 'del' | 'add';
  text: string;
}

export interface InstallPreview {
  path: string;
  exists: boolean;
  before: string | null;
  command: string;
  changed: boolean;
  installed: boolean;
  shim_found: boolean;
  diff: DiffLine[];
}

export interface RemoveResult {
  path: string;
  exists: boolean;
  before: string | null;
  restored: string | null;
  changed: boolean;
  diff: DiffLine[];
}

export const sourcesView = () => invoke<SourcesView>('sources_view');
export const sourcesStatus = () => invoke<Status>('sources_status');
export const sourcesSave = (config: Config) => invoke<SourcesView>('sources_save', { config });
export const setManagementKey = (key: string) => invoke<string | null>('sources_set_key', { key });
export const clearManagementKey = () => invoke<void>('sources_clear_key');
export const refreshSource = (source: 'claude-status' | 'cliproxyapi') => invoke<void>('sources_refresh', { source });
export const useLayout = (json: string) => invoke<void>('sources_use_layout', { json });
export const statuslinePreview = (settings: string | null) => invoke<InstallPreview>('statusline_preview', { settings });
export const statuslineInstall = (settings: string | null) => invoke<InstallPreview>('statusline_install', { settings });
export const statuslineRemove = (settings: string | null) => invoke<RemoveResult>('statusline_remove', { settings });

export function ago(unixMs: number | null, now = Date.now()): string {
  if (unixMs === null) return 'never';
  const seconds = Math.max(0, Math.round((now - unixMs) / 1000));
  if (seconds < 90) return `${seconds} s ago`;
  const minutes = Math.round(seconds / 60);
  return minutes < 90 ? `${minutes} min ago` : `${Math.round(minutes / 60)} h ago`;
}
