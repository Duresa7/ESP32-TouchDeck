import { useId } from 'react';
import clsx from 'clsx';
import { useStore } from 'zustand';
import type { DeviceSummary } from '../../companion/api.ts';
import { inProgress } from '../../lib/device/firmware.ts';
import { useDeviceStores } from '../../screens/layout/device.ts';
import { button } from '../button.ts';
import { nameOf } from './troubleshooting.ts';

function FileRow({ device }: { device: DeviceSummary }) {
  const { firmware } = useDeviceStores(device);
  const view = useStore(firmware, (s) => s.view);
  const log = useStore(firmware, (s) => s.log);
  const fileDialogOpen = useStore(firmware, (s) => s.fileDialogOpen);
  const name = nameOf(device);
  const ready = device.link.state === 'connected' && !fileDialogOpen && !inProgress(view);
  return (
    <div className="row firmware-file" data-testid="firmware-file" data-device={device.id} data-step={view.step}>
      <span className="row-text">
        <span className="row-title">{name}</span>
        <span
          className={clsx(view.step === 'failed' ? 'row-problem' : 'row-sub', view.step === 'running' && 'tabular', view.step === 'done' && 'is-ok')}
          data-testid="firmware-file-status"
        >
          {view.step === 'idle' ? 'A .tdfw file from an Open TouchDeck release.' : view.text}
        </span>
        {view.step === 'running' && (
          <span className="firmware-progress" role="progressbar" aria-label={`Updating ${name}`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={view.pct ?? undefined}>
            <span className="firmware-progress-fill" style={{ transform: `scaleX(${(view.pct ?? 0) / 100})` }} />
          </span>
        )}
        {(view.step === 'failed' || view.step === 'cancelled') && (
          <details className="row-details">
            <summary>Details</summary>
            <p>{log.join('\n')}</p>
          </details>
        )}
      </span>
      <button type="button" className={button()} aria-disabled={!ready} onClick={() => ready && void firmware.startFromFile()} data-testid="firmware-file-choose">
        Choose file
      </button>
    </div>
  );
}

export function FirmwareFile({ devices }: { devices: DeviceSummary[] }) {
  const id = useId();
  if (!devices.length) return null;
  return (
    <section className="settings-section" aria-labelledby={id}>
      <h3 id={id}>Update from a file</h3>
      <div className="group">
        {devices.map((device) => (
          <FileRow key={device.id} device={device} />
        ))}
      </div>
    </section>
  );
}
