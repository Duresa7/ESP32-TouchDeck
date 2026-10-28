import clsx from 'clsx';
import { Cable } from 'lucide-react';
import type { DeviceSummary } from '../companion/api.ts';
import { useDevices } from '../companion/devices.ts';
import { button } from '../shell/button.ts';
import { useUi } from '../state/ui.ts';

// The card's status words, from docs/copy.md, Home. Compared as a string so a
// link state the Companion adds later reads as "Not connected".
function statusOf(device: DeviceSummary): { words: string; live: boolean } {
  const state: string = device.link.state;
  switch (state) {
    case 'connected':
      return { words: 'Connected', live: true };
    case 'connecting':
      return { words: 'Connecting…', live: false };
    case 'busy':
      return { words: 'In use by another program', live: false };
    case 'updating':
      return { words: 'Updating…', live: false };
    default:
      return { words: 'Not connected', live: false };
  }
}

function DeviceCard({ device }: { device: DeviceSummary }) {
  const status = statusOf(device);
  const name = device.name ?? 'Unnamed Device';
  return (
    <li className="device-card">
      <span className="device-name" title={name}>
        {name}
      </span>
      <span className={clsx('status', status.live && 'is-live')}>
        <i className="led" aria-hidden />
        {status.words}
      </span>
    </li>
  );
}

function NoDevices() {
  const openSettings = useUi((s) => s.setSettingsOpen);
  return (
    <div className="empty">
      <h1 className="empty-title">Plug in your Device</h1>
      <p className="empty-text">Connect an Open TouchDeck to this PC with a USB data cable.</p>
      <button type="button" className={button()} onClick={() => openSettings(true)}>
        <Cable size={16} strokeWidth={1.9} aria-hidden />
        Connection help
      </button>
    </div>
  );
}

/** Home, the first screen: every Device by name, with its status. #81 builds the real one. */
export function Home() {
  const devices = useDevices((s) => s.devices);
  const live = useDevices((s) => s.status === 'live');
  return (
    <main className="home">
      {devices.length > 0 ? (
        <ul className="device-cards" aria-label="Devices">
          {devices.map((d) => (
            <DeviceCard key={d.id} device={d} />
          ))}
        </ul>
      ) : (
        live && <NoDevices />
      )}
    </main>
  );
}
