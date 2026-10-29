import { mkdir, mkdtemp, open, readFile, rm, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { compareVersions, verifyInstaller } from './signature';
import type { UpdateCheck, UpdateDownload, UpdateInstall, UpdateProgress } from './types';

export const UPDATE_FEED = 'https://github.com/Duresa7/ESP32-TouchDeck/releases/latest/download/latest.json';
export const INSTALLER_ARGS = ['/P', '/UPDATE', '/R'];
const MAX_DOWNLOAD = 256 * 1024 * 1024;
class InvalidDownload extends Error {}
class SignatureFailure extends Error {}
export type SystemCall = (method: string, path: string) => Promise<{ status: number; body: unknown }>;
type Options = {
  publicKey: string; current: string; dataDir: string; feed?: string; allowLoopback?: boolean;
  system: () => SystemCall; launch: (path: string, args: string[]) => Promise<void>; quit: () => void;
};
type Release = { version: string; url: string; signature: string; size?: number };

export function feedSettings(packaged: boolean, env = process.env) {
  const override = !packaged && env.TOUCHDECK_UPDATE_TEST_FEED;
  if (override) {
    const u = new URL(override);
    if (u.protocol !== 'http:' || u.hostname !== '127.0.0.1' || u.username || u.password) throw new Error('Invalid test feed');
    return { feed: u.href, allowLoopback: true };
  }
  return { feed: UPDATE_FEED, allowLoopback: false };
}

function optionalText(value: unknown): string | null { return typeof value === 'string' ? value : null; }

function field(value: unknown, name: string): unknown {
  return typeof value === 'object' && value !== null ? Reflect.get(value, name) : undefined;
}

export async function builtInKey(dist: string): Promise<string> {
  try {
    const value: unknown = JSON.parse(await readFile(join(dist, 'updater-key.json'), 'utf8'));
    const key = field(value, 'publicKey');
    return typeof key === 'string' ? key : '';
  } catch { return ''; }
}

export class Updater {
  private release?: Release;
  private ready?: { path: string; release: Release };
  private active = false;
  constructor(private readonly options: Options) {}

  private validateUrl(value: string): URL {
    const url = new URL(value);
    if (url.username || url.password || (url.protocol !== 'https:' &&
        !(this.options.allowLoopback && url.protocol === 'http:' && url.hostname === '127.0.0.1'))) throw new Error('Unsafe update URL');
    return url;
  }

  private async response(value: string, signal: AbortSignal): Promise<{ body: ReadableStream<Uint8Array>; headers: Headers }> {
    let url = this.validateUrl(value);
    for (let i = 0; i < 6; i++) {
      const res = await fetch(url, { redirect: 'manual', signal, headers: { 'Accept-Encoding': 'identity' } });
      if ([301, 302, 303, 307, 308].includes(res.status)) {
        await res.body?.cancel();
        url = this.validateUrl(new URL(res.headers.get('location') ?? '', url).href);
        continue;
      }
      if (!res.ok || !res.body || res.headers.get('content-encoding')) { await res.body?.cancel(); throw new Error('Invalid response'); }
      return { body: res.body, headers: res.headers };
    }
    throw new Error('Too many redirects');
  }

  private async small(value: string): Promise<Buffer> {
    const res = await this.response(value, AbortSignal.timeout(15000));
    const reader = res.body.getReader();
    const chunks: Uint8Array[] = []; let size = 0;
    try {
      for (;;) {
        const item = await reader.read(); if (item.done) break;
        size += item.value.length;
        if (size > 1024 * 1024) throw new Error('Manifest too large');
        chunks.push(item.value);
      }
      return Buffer.concat(chunks);
    } finally { await reader.cancel().catch(() => {}); }
  }

  async check(): Promise<UpdateCheck> {
    if (!this.options.publicKey) return { state: 'disabled' };
    if (this.active) return { state: 'failed' };
    this.active = true;
    try {
      this.release = undefined;
      const manifest: unknown = JSON.parse((await this.small(this.options.feed ?? UPDATE_FEED)).toString('utf8'));
      const version = field(manifest, 'version');
      if (typeof version !== 'string') throw new Error('Invalid manifest');
      if (compareVersions(version, this.options.current) <= 0) return { state: 'none' };
      const entry = field(field(manifest, 'platforms'), 'windows-x86_64');
      const url = field(entry, 'url'), signature = field(entry, 'signature'), size = field(entry, 'size');
      if (!entry || typeof url !== 'string' || typeof signature !== 'string' || signature.length > 16384 ||
          (size !== undefined && (typeof size !== 'number' || !Number.isSafeInteger(size) || size <= 0 || size > MAX_DOWNLOAD))) throw new Error('Invalid platform');
      this.validateUrl(url);
      this.release = { version, url, signature, size };
      return { state: 'available', version, current: this.options.current,
        notes: optionalText(field(manifest, 'notes')), date: optionalText(field(manifest, 'pub_date')) };
    } catch (error) { return { state: error instanceof TypeError || (error instanceof Error && error.name === 'TimeoutError') ? 'offline' : 'failed' }; }
    finally { this.active = false; }
  }

  private async verify(path: string, release: Release) {
    try {
      const comment = await verifyInstaller(path, release.signature, this.options.publicKey);
      const signed = comment.split('\t').find(field => field.startsWith('version:'))?.slice(8);
      if (!signed || compareVersions(signed, release.version) !== 0) throw new Error('Signed version mismatch');
    } catch { throw new SignatureFailure(); }
  }

  async download(progress: (p: UpdateProgress) => void): Promise<UpdateDownload> {
    if (this.active || !this.release || !this.options.publicKey) return { state: 'failed', reason: 'download' };
    this.active = true;
    let dir: string | undefined;
    let reading = false;
    try {
      if (this.ready) { await rm(this.ready.path, { force: true }); this.ready = undefined; }
      const release = this.release;
      await mkdir(this.options.dataDir, { recursive: true });
      dir = await mkdtemp(join(this.options.dataDir, 'update-'));
      const path = join(dir, 'setup.exe');
      const response = await this.response(release.url, AbortSignal.timeout(5 * 60 * 1000));
      const length = response.headers.get('content-length');
      const total = length === null ? null : Number(length);
      if (total !== null && (!Number.isSafeInteger(total) || total <= 0 || total > MAX_DOWNLOAD)) {
        await response.body?.cancel(); throw new InvalidDownload();
      }
      const file = await open(path, 'wx', 0o600);
      const reader = response.body.getReader();
      let downloaded = 0;
      reading = true;
      try {
        for (;;) {
          const item = await reader.read(); if (item.done) break;
          downloaded += item.value.length;
          if (downloaded > MAX_DOWNLOAD || (total !== null && downloaded > total) || (release.size !== undefined && downloaded > release.size)) throw new InvalidDownload();
          await file.writeFile(item.value);
          try { progress({ downloaded, total: release.size ?? total }); } catch { }
        }
        if ((total !== null && downloaded !== total) || (release.size !== undefined && downloaded !== release.size)) throw new SignatureFailure();
      } finally { await reader.cancel().catch(() => {}); await file.close(); }
      reading = false;
      await this.verify(path, release);
      this.ready = { path, release };
      return { state: 'ready' };
    } catch (error) {
      if (dir) await rm(dir, { recursive: true, force: true }).catch(() => {});
      if (error instanceof SignatureFailure || (reading && !(error instanceof InvalidDownload))) return { state: 'failed', reason: 'signature' };
      return error instanceof TypeError || (error instanceof Error && error.name === 'TimeoutError')
        ? { state: 'offline' } : { state: 'failed', reason: 'download' };
    } finally { this.active = false; }
  }

  async install(): Promise<UpdateInstall> {
    if (this.active || !this.ready) return { state: 'failed', reason: 'installer' };
    this.active = true;
    let lock: string | undefined;
    const system = this.options.system();
    try {
      const ready = this.ready;
      if ((await stat(ready.path)).size > MAX_DOWNLOAD) throw new Error('Oversized installer');
      await this.verify(ready.path, ready.release);
      const acquired = await system('POST', '/system/lock');
      const error = field(acquired.body, 'error');
      if (acquired.status === 409 && field(error, 'code') === 'system.busy') {
        const device = field(field(error, 'details'), 'device');
        return { state: 'failed', reason: 'busy', device: typeof device === 'string' ? device : null };
      }
      const acquiredLock = field(acquired.body, 'lock');
      if (acquired.status !== 200 || typeof acquiredLock !== 'string' || !acquiredLock) throw new Error('Lock unavailable');
      lock = acquiredLock;
      const renewed = await system('PUT', `/system/lock/${encodeURIComponent(lock)}`);
      if (renewed.status < 200 || renewed.status >= 300) throw new Error('Lock lost');
      const stopped = await system('POST', '/system/shutdown');
      if (stopped.status < 200 || stopped.status >= 300) throw new Error('Shutdown failed');
      await this.options.launch(ready.path, [...INSTALLER_ARGS]);
      this.options.quit();
      return await new Promise<never>(() => {});
    } catch (error) {
      if (error instanceof SignatureFailure && this.ready) {
        await rm(this.ready.path, { force: true }).catch(() => {}); this.ready = undefined;
      }
      return { state: 'failed', reason: 'installer' };
    } finally {
      if (lock) await system('DELETE', `/system/lock/${encodeURIComponent(lock)}`).catch(() => {});
      this.active = false;
    }
  }
}
