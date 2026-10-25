// A stand-in for CLIProxyAPI's management API, for the sources specs and CI.
//
// It answers GET /v0/management/auth-files with the fixture when the bearer key matches
// and 401 otherwise, and keeps a log of every request it saw. Nothing else is served.
//
//   import { start } from './cliproxy-server.mjs';
//   const server = await start({ key: 'fixture-key' });   // server.url, server.requests
//
//   node cliproxy-server.mjs --port 8317 --key fixture-key     run it by hand
import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const AUTH_FILES = '/v0/management/auth-files';
export const DEFAULT_KEY = 'fixture-key-0123';
export const DEFAULT_FIXTURE = join(fileURLToPath(new URL('.', import.meta.url)), '..', '..', 'crates', 'td-host', 'tests', 'fixtures', 'auth-files.json');

/** Starts the server on `port`, or on a free one when `port` is 0, and resolves once it listens. */
export async function start({ port = 0, key = DEFAULT_KEY, fixture = DEFAULT_FIXTURE, host = '127.0.0.1' } = {}) {
  const body = readFileSync(fixture, 'utf8');
  const requests = [];
  const server = createServer((request, response) => {
    const bearer = request.headers.authorization ?? '';
    const authorized = bearer === `Bearer ${key}`;
    requests.push({ at: Date.now(), method: request.method, path: request.url, authorized });
    const send = (status, payload) => {
      response.writeHead(status, { 'content-type': 'application/json', 'content-length': Buffer.byteLength(payload) });
      response.end(payload);
    };
    if (request.method !== 'GET') return send(405, '{"error":"method not allowed"}');
    if (request.url !== AUTH_FILES) return send(404, '{"error":"not found"}');
    if (!authorized) return send(401, '{"error":"missing management key"}');
    send(200, body);
  });
  server.keepAliveTimeout = 0;
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, host, resolve);
  });
  const address = server.address();
  return {
    url: `http://${host}:${address.port}`,
    port: address.port,
    requests,
    stop: () =>
      new Promise((resolve) => {
        server.closeAllConnections();
        server.close(() => resolve());
      }),
  };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const option = (name, fallback) => {
    const at = process.argv.indexOf(name);
    return at >= 0 ? process.argv[at + 1] : fallback;
  };
  const server = await start({ port: Number(option('--port', 8317)), key: option('--key', DEFAULT_KEY), fixture: option('--fixture', DEFAULT_FIXTURE) });
  console.log(JSON.stringify({ url: server.url, key: option('--key', DEFAULT_KEY) }));
}
