import clsx from 'clsx';
import { ArrowUpCircle, Cable } from 'lucide-react';
import type { DeviceSummary } from '../companion/api.ts';
import { useDevices } from '../companion/devices.ts';
import { button } from '../shell/button.ts';
import { useUi } from '../state/ui.ts';
import { DevicePreview } from './DevicePreview.tsx';
import { deviceWidth, firmwareUpdate, lastSeen, previewOf, statusOf } from './home-model.ts';
import './home.css';

function DeviceCard({ device, width }: { device: DeviceSummary; width: number }) {
  const openDevice = useUi((s) => s.openDevice);
  const status = statusOf(device.link.state);
  const name = device.name ?? 'Unnamed Device';
  const seen = !status.live && lastSeen(device);
  return <li className="device-figure" style={{ width }}>
    <button type="button" className="device-hit" title="Edit Layout" aria-label={`Edit ${name}`} onClick={() => openDevice(device.id)}>
      <DevicePreview preview={previewOf(device.layout)} live={status.live} />
    </button>
    <div className="device-caption">
      <span className="device-name" title={name}>{name}</span>
      <span className="device-meta">
        <span className={clsx('status', status.live && 'is-live')}><i className="led" aria-hidden />{status.words}</span>
        {seen && <span className="device-seen">{seen}</span>}
        {firmwareUpdate(device) && <span className="badge-signal"><ArrowUpCircle size={14} aria-hidden />Firmware update</span>}
      </span>
    </div>
  </li>;
}

function NoDevices() {
  const openHelp = useUi((s) => s.openTroubleshooting);
  return <div className="empty">
    <div className="device-ghost"><DevicePreview preview={null} live={false} /></div>
    <h1 className="empty-title">Plug in your Device</h1>
    <p className="empty-text">Connect an Open TouchDeck to this PC with a USB data cable.</p>
    <p className="empty-text quiet">New board? Plug it in the same way and you’ll be offered a Firmware install.</p>
    <button type="button" className={button()} onClick={openHelp}><Cable size={16} strokeWidth={1.9} aria-hidden />Connection help</button>
  </div>;
}

export function Home() {
  const devices = useDevices((s) => s.devices);
  const live = useDevices((s) => s.status === 'live');
  return <main className={clsx('home', devices.length === 1 && 'home-single')}>
    {devices.length ? <ul className="device-row" aria-label="Devices">
      {devices.map((device) => <DeviceCard key={device.id} device={device} width={deviceWidth(devices.length)} />)}
    </ul> : live && <NoDevices />}
  </main>;
}
