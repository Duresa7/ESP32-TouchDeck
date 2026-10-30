import { useEffect, useMemo } from 'react';
import clsx from 'clsx';
import { ArrowUpCircle, Cable } from 'lucide-react';
import type { DeviceSummary } from '../companion/api.ts';
import { useCompanion } from '../companion/connection.ts';
import { useDevices } from '../companion/devices.ts';
import { button } from '../shell/button.ts';
import { useUi } from '../state/ui.ts';
import { DevicePreview } from './DevicePreview.tsx';
import { useDeviceLayout } from './home-layouts.ts';
import { deviceWidth, firmwareUpdate, lastSeen, orientationOf, previewOf, statusOf } from './home-model.ts';
import { boardCards, boardInstallFor, boardsUi, useBoardsUi } from './new-boards.ts';
import { IgnoredBoards, NewBoardCard } from './NewBoardCard.tsx';
import './home.css';

function DeviceCard({ device, width }: { device: DeviceSummary; width: number }) {
  const openDevice = useUi((s) => s.openDevice);
  const status = statusOf(device.link.state);
  const name = device.name ?? 'Unnamed Device';
  const seen = !status.live && lastSeen(device);
  const layout = useDeviceLayout(device);
  return <li className="device-figure" style={{ width }}>
    <button type="button" className="device-hit" title="Edit Layout" aria-label={`Edit ${name}`} onClick={() => openDevice(device.id)}>
      <DevicePreview preview={previewOf(layout)} live={status.live} orientation={orientationOf(device)} />
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

function useHandedOff(devices: DeviceSummary[]) {
  const companion = useCompanion();
  const started = useBoardsUi((s) => s.started);
  useEffect(() => {
    for (const device of devices) {
      if (!started[device.id]) continue;
      boardsUi.getState().end(device.id);
      boardInstallFor(companion, device).dismiss();
    }
  }, [companion, devices, started]);
}

export function Home() {
  const devices = useDevices((s) => s.devices);
  const newBoards = useDevices((s) => s.newBoards);
  const live = useDevices((s) => s.status === 'live');
  const ignored = useBoardsUi((s) => s.ignored);
  const started = useBoardsUi((s) => s.started);
  const boards = useMemo(() => boardCards(newBoards, devices, { ignored, started }), [newBoards, devices, ignored, started]);
  useHandedOff(devices);
  const count = devices.length + boards.shown.length;
  const width = deviceWidth(count);
  return <main className={clsx('home', count === 1 && 'home-single')}>
    {count ? <ul className="device-row" aria-label="Devices">
      {devices.map((device) => <DeviceCard key={device.id} device={device} width={width} />)}
      {boards.shown.map((board) => <NewBoardCard key={board.id} board={board} width={width} />)}
    </ul> : live && <NoDevices />}
    <IgnoredBoards boards={boards.ignored} />
  </main>;
}
