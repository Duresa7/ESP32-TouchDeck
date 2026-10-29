import { useEffect, useState, type KeyboardEvent } from 'react';
import type { DeviceKeyMirror } from '../../companion/api.ts';
import { actionsStore, useActions } from '../../companion/actions.ts';
import { button } from '../../shell/button.ts';

const EVENTS_KEPT = 2000;
const NONE: DeviceKeyMirror[] = [];

interface Typed {
  type: 'down' | 'up';
  key: string;
  at: number;
}

function reportLine(report: DeviceKeyMirror) {
  const gap = report.gap ? ` (missed ${report.gap[0]} to ${report.gap[1]}, released all keys)` : '';
  const outcome = report.outcome === 'typed' ? '' : ` (${report.outcome})`;
  return `${report.report}${gap}${outcome}`;
}

export function KeyTest({ deviceId, device }: { deviceId: string; device: string }) {
  const reports = useActions((s) => s.mirror[deviceId]) ?? NONE;
  const [typed, setTyped] = useState<Typed[]>([]);
  useEffect(() => actionsStore.getState().watchKeys(deviceId), [deviceId]);

  // F13 and the media keys type no text, so each key goes in by name, and every keydown and keyup is kept apart.
  const record = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    event.preventDefault();
    const type = event.type === 'keydown' ? 'down' : 'up';
    setTyped((list) => [...list.slice(-(EVENTS_KEPT - 1)), { type, key: event.key, at: performance.timeOrigin + performance.now() }]);
  };
  const text = typed
    .filter((event) => event.type === 'down')
    .map((event) => event.key)
    .join(' ');
  const clear = () => {
    setTyped([]);
    actionsStore.getState().clearMirror(deviceId);
  };

  return (
    <div className="key-test" data-testid="key-test">
      <p className="field-hint">Click the box, then tap Tiles on {device}. Each key it sends to this PC shows here.</p>
      <div className="key-test-row">
        <textarea
          readOnly
          className="input textarea key-test-box"
          value={text}
          onKeyDown={record}
          onKeyUp={record}
          onFocus={() => {
            if (!document.hasFocus()) window.focus();
          }}
          placeholder={`Click here, then tap a Tile on ${device}`}
          aria-label="Key test"
          data-testid="key-monitor"
        />
        <button type="button" className={button({ size: 'small' })} onClick={clear} data-testid="key-monitor-clear">
          Clear
        </button>
      </div>
      <div className="key-test-lists">
        <div>
          <h4 className="field-label">Sent by {device}</h4>
          <ol className="key-list" data-testid="key-reports">
            {reports.map((report) => (
              <li key={`${report.seq}-${report.arrived_ms}`} data-testid="key-report" data-outcome={report.outcome} data-seq={report.seq}>
                {reportLine(report)}
              </li>
            ))}
          </ol>
        </div>
        <div>
          <h4 className="field-label">Typed on this PC</h4>
          <ol className="key-list" data-testid="key-events">
            {typed.map((event, i) => (
              <li key={i} data-testid="key-event" data-type={event.type} data-key={event.key}>
                {event.type} {event.key}
              </li>
            ))}
          </ol>
        </div>
      </div>
    </div>
  );
}
