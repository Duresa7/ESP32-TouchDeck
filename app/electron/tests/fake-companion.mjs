import { createServer } from 'node:http';
import { createHmac, randomBytes } from 'node:crypto';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export async function fakeCompanion({ wrongProof = false, api = ['v1'] } = {}) {
  const dir = await mkdtemp(join(tmpdir(), 'touchdeck-shell-'));
  const pairing = 'tdp_' + randomBytes(32).toString('base64url');
  const sessions = new Set();
  const calls = [];
  const server = createServer(async (req, res) => {
    let text = '';
    for await (const chunk of req) text += chunk;
    const body = text ? JSON.parse(text) : {};
    calls.push({ method: req.method, path: req.url, authorization: req.headers.authorization, body });
    res.setHeader('Content-Type', 'application/json');
    if (req.url === '/api/v1/system/ping') return res.end(JSON.stringify({ api }));
    if (req.url === '/api/v1/auth/proof' && req.method === 'POST') {
      const nonce = Buffer.from(body.nonce, 'base64url');
      if (nonce.length !== 32) { res.statusCode = 400; return res.end('{}'); }
      const proof = createHmac('sha256', wrongProof ? 'wrong' : pairing)
        .update('touchdeck-companion-proof\0').update(nonce).digest('base64url');
      return res.end(JSON.stringify({ proof }));
    }
    if (req.url === '/api/v1/auth/sessions' && req.method === 'POST' &&
        req.headers.authorization === `Bearer ${pairing}` && body.kind === 'window') {
      const token = 'tds_' + randomBytes(32).toString('base64url');
      sessions.add(token);
      return res.end(JSON.stringify({ id: randomBytes(8).toString('hex'), token }));
    }
    if (req.url === '/api/v1/auth/sessions/self' && req.method === 'DELETE' &&
        sessions.delete(req.headers.authorization?.slice(7))) {
      res.statusCode = 204; return res.end();
    }
    if (!req.url.startsWith('/api/')) {
      res.setHeader('Content-Type', 'text/html');
      res.setHeader('Content-Security-Policy', "default-src 'self'; script-src 'self'; object-src 'none'; base-uri 'none'");
      return res.end('<!doctype html><title>Open TouchDeck</title><h1>Open TouchDeck</h1><p>Companion shell test</p>');
    }
    res.statusCode = 401;
    res.end(JSON.stringify({ error: { code: 'auth.revoked', message: 'Session unavailable', details: null } }));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const port = server.address().port;
  const runtime = { version: 1, api, port, token: pairing, pid: process.pid, build: 'test', started_at_ms: Date.now() };
  const path = join(dir, 'runtime', 'companion.json');
  await mkdir(join(dir, 'runtime'));
  await writeFile(path, JSON.stringify(runtime));
  return { dir, path, runtime, calls, sessions, origin: `http://127.0.0.1:${port}`,
    async close() { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); await rm(dir, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 }); } };
}
