export const AUTH_FILES: string;
export const API_CALL: string;
export const CODEX_USAGE: string;
export const CLAUDE_USAGE: string;
export const DEFAULT_KEY: string;
export const DEFAULT_FIXTURE: string;

export interface LoggedRequest {
  at: number;
  method: string | undefined;
  path: string | undefined;
  authorized: boolean;
  headers: Record<string, string | string[] | undefined>;
  body: { auth_index?: string; method?: string; url?: string; header?: Record<string, string> } | null;
}

export interface FixtureServer {
  url: string;
  port: number;
  requests: LoggedRequest[];
  setFiveHourPercent(percent: number): void;
  stop(): Promise<void>;
}

export function start(options?: { port?: number; key?: string; fixture?: string; host?: string }): Promise<FixtureServer>;
