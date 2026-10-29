import type { Environment } from './connection.ts';
import { createFakeCompanion } from './fake.ts';
import { demoDevices } from './samples.ts';
import { windowSession } from './session.ts';

// `pnpm dev:ui:demo` runs the UI against the fake Companion. The generated client
// calls the global fetch, so API requests are routed to the fake here; everything
// else, such as Vite's own requests, still goes to the network.
export function demoEnvironment(target: Window): Environment {
  const fake = createFakeCompanion({ devices: demoDevices, system: { build: 'demo' } });
  const network = target.fetch.bind(target);
  target.fetch = (input, init) => {
    const url = new URL(input instanceof Request ? input.url : String(input), target.location.href);
    return url.origin === target.location.origin && url.pathname.startsWith('/api/') ? fake.fetch(input, init) : network(input, init);
  };
  return { base: target.location.origin, session: windowSession(fake.bridge), open: fake.openSocket };
}
