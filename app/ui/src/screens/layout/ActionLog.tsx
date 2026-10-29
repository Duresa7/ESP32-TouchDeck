import { useEffect } from 'react';
import type { ActionLog } from '../../companion/actions-api.ts';
import { actionsStore, useActions } from '../../companion/actions.ts';
import { useCompanion } from '../../companion/connection.ts';
import { useDevices } from '../../companion/devices.ts';
import { plural } from '../../lib/editor/messages.ts';
import type { Layout } from '../../lib/editor/validate.ts';

const NONE: ActionLog[] = [];

function outcome(entry: ActionLog, device: string): string {
  switch (entry.status) {
    case 'started':
      return entry.pid === null ? 'Started' : `Started (process ${entry.pid})`;
    case 'exited':
      return entry.exit_code === 0 ? 'Finished' : `Exited with code ${entry.exit_code ?? '?'}`;
    case 'denied':
      return 'Denied';
    case 'failed':
      return 'Failed';
    case 'unknown':
      return `Ignored, not in ${device}'s Layout`;
    case 'expired':
      return 'Expired, no answer on this PC';
    default:
      return entry.status;
  }
}

function reported(layout: unknown): { name: string; pages: number; tiles: number } | null {
  if (typeof layout !== 'object' || layout === null || !('name' in layout) || !('pages' in layout) || !('tiles' in layout)) return null;
  const { name, pages, tiles } = layout;
  return typeof name === 'string' && typeof pages === 'number' && typeof tiles === 'number' ? { name, pages, tiles } : null;
}

function tileLabel(layout: Layout, page: string, tile: string) {
  const found = layout.pages.find((p) => p.id === page);
  return (found?.kind === 'grid' ? found.tiles.find((t) => t.id === tile)?.label : undefined) ?? tile;
}

export function ActionLogPanel({ deviceId, device, layout }: { deviceId: string; device: string; layout: Layout }) {
  const companion = useCompanion();
  const entries = useActions((s) => s.log[deviceId]) ?? NONE;
  const summary = reported(useDevices((s) => s.devices.find((d) => d.id === deviceId)?.layout));
  useEffect(() => {
    let live = true;
    companion.actions.log({}, { device: deviceId }).then(
      (log) => live && actionsStore.getState().loadLog(deviceId, log),
      () => {},
    );
    return () => {
      live = false;
    };
  }, [companion, deviceId]);
  const newest = [...entries].reverse();
  return (
    <div className="action-log" data-testid="actions">
      <p className="field-hint">What happened each time a Tile opened an app or ran a command.</p>
      {summary && (
        <p className="field-hint" data-testid="action-layout">
          Using “{summary.name}” from {device}: {plural(summary.pages, 'Page')}, {plural(summary.tiles, 'Tile')}
        </p>
      )}
      <ol className="log-list" data-testid="action-log">
        {newest.map((entry) => (
          <li key={entry.id} data-testid="action-entry" data-tile={entry.tile} data-status={entry.status} data-exit={entry.exit_code ?? ''} data-pid={entry.pid ?? ''}>
            <span className="log-when tabular">{new Date(entry.at_ms).toLocaleTimeString()}</span>
            <span className="log-what">
              <strong>{tileLabel(layout, entry.page, entry.tile)}</strong> {outcome(entry, device)}
            </span>
            {entry.status === 'failed' && entry.detail && (
              <details className="log-details">
                <summary>Details</summary>
                <p>{entry.detail}</p>
              </details>
            )}
          </li>
        ))}
        {newest.length === 0 && <li className="log-none">No actions yet. Tap a Tile that opens an app or runs a command.</li>}
      </ol>
    </div>
  );
}
