import type { Environment } from './connection.ts';
import { createFakeCompanion } from './fake.ts';
import { demoLayout, demoScenario } from './samples.ts';
import { windowSession } from './session.ts';

// `pnpm dev:ui:demo` runs the UI against the fake Companion. The generated client
// calls the global fetch, so API requests are routed to the fake here; everything
// else, such as Vite's own requests, still goes to the network.
export function demoEnvironment(target: Window): Environment {
  const requested = Number(new URL(target.location.href).searchParams.get('devices') ?? 3);
  const count = Number.isInteger(requested) && requested >= 0 && requested <= 5 ? requested : 3;
  const devices = demoScenario(count);
  const fake = createFakeCompanion({ devices, layouts: Object.fromEntries(devices.map((d) => [d.id, demoLayout()])), system: { build: 'demo' } });
  // Demo-only controls exercise the real event path, never the UI store directly.
  Object.assign(target, { touchdeckDemo: {
    push: (id: string, order: string[]) => fake.setLayout(id, demoLayout(order)),
    offline: (id: string) => fake.update(id, { link: { state: 'lost', reason: 'unplugged', detail: '', at_ms: Date.UTC(2026, 9, 27, 21, 40) } }),
  } });
  const network = target.fetch.bind(target);
  target.fetch = (input, init) => {
    const url = new URL(input instanceof Request ? input.url : String(input), target.location.href);
    return url.origin === target.location.origin && url.pathname.startsWith('/api/') ? fake.fetch(input, init) : network(input, init);
  };
  return { base: target.location.origin, session: windowSession(fake.bridge), open: fake.openSocket };
}
