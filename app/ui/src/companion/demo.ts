import type { DeviceSummary } from './api.ts';
import type { Environment } from './connection.ts';
import { createFakeCompanion } from './fake.ts';
import { createFakeUpdates, sampleOffer, type FakeUpdates } from './fake-updates.ts';
import { demoLayout, demoScenario } from './samples.ts';
import { windowSession, type UpdateCheck } from './session.ts';

const checks = {
  none: { state: 'none' },
  available: sampleOffer,
  'no-notes': { ...sampleOffer, notes: null },
  disabled: { state: 'disabled' },
  offline: { state: 'offline' },
  failed: { state: 'failed' },
} satisfies Record<string, UpdateCheck>;

const isCheckName = (name: string | null): name is keyof typeof checks => name !== null && Object.hasOwn(checks, name);

declare global {
  interface Window {
    touchdeckDemo?: {
      push(id: string, order: string[]): void;
      offline(id: string): void;
      device(id: string, change: Partial<Omit<DeviceSummary, 'id' | 'rev'>>): void;
      firmware: { hold(id: string): void; release(id: string): void };
      updates: Pick<FakeUpdates, 'script' | 'finish' | 'calls'>;
    };
  }
}

export function demoEnvironment(target: Window): Environment {
  const params = new URL(target.location.href).searchParams;
  const requested = Number(params.get('devices') ?? 3);
  const count = Number.isInteger(requested) && requested >= 0 && requested <= 5 ? requested : 3;
  const devices = demoScenario(count);
  const fake = createFakeCompanion({
    devices,
    layouts: Object.fromEntries(devices.map((d) => [d.id, demoLayout()])),
    system: { build: 'demo' },
    brightness: Object.fromEntries(devices.filter((_, i) => i % 3 !== 2).map((d) => [d.id, 80])),
  });
  const update = params.get('update');
  const updates = createFakeUpdates({ check: isCheckName(update) ? checks[update] : checks.none });
  target.touchdeckDemo = {
    push: (id, order) => fake.setLayout(id, demoLayout(order)),
    offline: (id) => fake.update(id, { link: { state: 'lost', reason: 'unplugged', detail: '', at_ms: Date.UTC(2026, 9, 27, 21, 40) } }),
    device: (id, change) => fake.update(id, change),
    firmware: { hold: fake.firmware.hold, release: fake.firmware.release },
    updates: { script: updates.script, finish: updates.finish, calls: updates.calls },
  };
  const network = target.fetch.bind(target);
  target.fetch = (input, init) => {
    const url = new URL(input instanceof Request ? input.url : String(input), target.location.href);
    return url.origin === target.location.origin && url.pathname.startsWith('/api/') ? fake.fetch(input, init) : network(input, init);
  };
  return { base: target.location.origin, session: windowSession(fake.bridge), open: fake.openSocket, updates: updates.bridge };
}
