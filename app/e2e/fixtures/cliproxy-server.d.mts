export const AUTH_FILES: string;
export const DEFAULT_KEY: string;
export const DEFAULT_FIXTURE: string;

export interface LoggedRequest {
  at: number;
  method: string | undefined;
  path: string | undefined;
  authorized: boolean;
}

export interface FixtureServer {
  url: string;
  port: number;
  requests: LoggedRequest[];
  stop(): Promise<void>;
}

export function start(options?: { port?: number; key?: string; fixture?: string; host?: string }): Promise<FixtureServer>;
