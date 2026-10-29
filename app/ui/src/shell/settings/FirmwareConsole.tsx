import { useEffect, useId, useLayoutEffect, useRef, useState, type FormEvent } from 'react';
import { Toggle } from '@base-ui/react/toggle';
import { ToggleGroup } from '@base-ui/react/toggle-group';
import { useStore } from 'zustand';
import type { DeviceSummary } from '../../companion/api.ts';
import { consoleCommands } from '../../companion/console.ts';
import { useCompanion, useEvents } from '../../companion/connection.ts';
import { button } from '../button.ts';
import { createConsoleStore, type ConsoleNotice } from './console.ts';
import { nameOf } from './troubleshooting.ts';

function Notice({ notice }: { notice: NonNullable<ConsoleNotice> }) {
  if (notice.kind === 'failed') {
    return (
      <p className="console-notice is-error" role="alert">
        {notice.text}
      </p>
    );
  }
  return (
    <div className="console-notice" role="status" data-testid="console-forbidden">
      <p>“{notice.line}” wasn't sent. The console takes only these read-only commands:</p>
      <ul className="console-commands" aria-label="Commands the console takes">
        {consoleCommands.map((command) => (
          <li key={command}>
            <code>{command}</code>
          </li>
        ))}
      </ul>
    </div>
  );
}

function Session({ device }: { device: DeviceSummary }) {
  const companion = useCompanion();
  const events = useEvents();
  const name = nameOf(device);
  const [store] = useState(() => createConsoleStore({ companion, device: device.id, name }));
  const entries = useStore(store, (s) => s.entries);
  const notice = useStore(store, (s) => s.notice);
  const [line, setLine] = useState('');
  const log = useRef<HTMLOListElement>(null);

  useEffect(() => events.console(device.id, (text) => store.getState().receive(text)), [events, device.id, store]);
  useLayoutEffect(() => {
    const list = log.current;
    if (list) list.scrollTop = list.scrollHeight;
  }, [entries]);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if ((await store.getState().send(line)) === 'sent') setLine('');
  }

  return (
    <div className="console">
      <p className="console-hint">
        Send a command to {name}'s console, like <code>version</code> or <code>layout state</code>.
      </p>
      <ol className="console-log" ref={log} aria-label="Console output" aria-live="polite" data-testid="console-log">
        {entries.length ? (
          entries.map((entry, i) => (
            <li key={i} className={`console-${entry.kind}`} data-kind={entry.kind}>
              {entry.kind === 'sent' ? `> ${entry.text}` : entry.text}
            </li>
          ))
        ) : (
          <li className="console-none">Nothing yet. Send a command below.</li>
        )}
      </ol>
      <form className="console-prompt" onSubmit={(event) => void submit(event)}>
        <input
          className="console-input"
          value={line}
          onChange={(event) => setLine(event.target.value)}
          placeholder="version"
          autoComplete="off"
          spellCheck={false}
          aria-label="Console command"
        />
        <button type="submit" className={button()} disabled={!line.trim()}>
          Send
        </button>
      </form>
      {notice && <Notice notice={notice} />}
    </div>
  );
}

export function FirmwareConsole({ devices }: { devices: DeviceSummary[] }) {
  const connected = devices.filter((device) => device.link.state === 'connected');
  const [chosen, setChosen] = useState<string | null>(null);
  const id = useId();
  const device = connected.find((d) => d.id === chosen) ?? connected[0];
  const first = devices[0];
  if (!first) return null;

  return (
    <section className="settings-section" aria-labelledby={`${id}-console`}>
      <h3 id={`${id}-console`}>Firmware console</h3>
      <div className="group">
        {connected.length > 1 && device && (
          <div className="row">
            <span className="row-title" id={`${id}-device`}>
              Device
            </span>
            <ToggleGroup
              value={[device.id]}
              onValueChange={(value) => value[0] && setChosen(value[0])}
              aria-labelledby={`${id}-device`}
              className="segmented"
            >
              {connected.map((d) => (
                <Toggle key={d.id} value={d.id} className="segment segment-name" title={nameOf(d)}>
                  {nameOf(d)}
                </Toggle>
              ))}
            </ToggleGroup>
          </div>
        )}
        {device ? (
          <Session key={device.id} device={device} />
        ) : (
          <div className="row">
            <span className="row-note">Plug in {nameOf(first)} to use the console.</span>
          </div>
        )}
      </div>
    </section>
  );
}
