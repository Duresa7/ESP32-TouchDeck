import { useUi } from '../state/ui.ts';
import { useDevices } from '../companion/devices.ts';

/** The Layout screen's place. #82 builds the editor. */
export function Layout() {
  const deviceId = useUi((s) => s.deviceId);
  const device = useDevices((s) => deviceId ? s.devices.find((d) => d.id === deviceId) : s.devices.find((d) => d.name !== null));
  return (
    <main className="screen" data-device-id={deviceId}>
      <header className="page-head">
        <h1 className="page-title">Layout</h1>
      </header>
      <div className="empty empty-start">
        {device ? (
          <>
            <h2 className="empty-title">Start a Layout</h2>
            <p className="empty-text">Choose what each Tile on {device.name} does.</p>
          </>
        ) : (
          <>
            <h2 className="empty-title">Plug in your Device</h2>
            <p className="empty-text">Connect an Open TouchDeck to this PC with a USB data cable.</p>
          </>
        )}
      </div>
    </main>
  );
}
