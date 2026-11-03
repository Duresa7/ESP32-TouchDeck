import type { ComponentType } from 'react';
import { Activity, Braces, Cpu, Gauge, Monitor, Music2, Pause, Play, Plug, Server, Type, Volume2, Wifi, type LucideProps } from 'lucide-react';
import type { DataStream, PresetTile } from '../../companion/api.ts';
import { presetLabel, presetLook } from '../../lib/integrations/model.ts';
import { Tip } from '../layout/Tip.tsx';
import '../../styles/integrations.css';

const GLYPHS: Partial<Record<string, ComponentType<LucideProps>>> = {
  activity: Activity,
  braces: Braces,
  code: Braces,
  cpu: Cpu,
  gauge: Gauge,
  monitor: Monitor,
  music: Music2,
  server: Server,
  volume: Volume2,
  wifi: Wifi,
};

/** An Integration's glyph from the icon name it reports. */
export function IntegrationGlyph({ icon, size = 20 }: { icon: string; size?: number }) {
  const Glyph = GLYPHS[icon] ?? Plug;
  return (
    <span className="integration-glyph" aria-hidden>
      <Glyph size={size} strokeWidth={1.9} />
    </span>
  );
}

const solid = { size: 11, strokeWidth: 0, fill: 'currentColor' } as const;

function PresetPreview({ preset, streams, needs }: { preset: PresetTile; streams: readonly DataStream[]; needs: string | null }) {
  const look = presetLook(preset, streams);
  return (
    <span className="preset" data-look={look} data-needs={needs ?? undefined}>
      {needs ? (
        <span className="preset-needs">{needs}</span>
      ) : (
        <span className="preset-art" aria-hidden>
          {look === 'bar' && (
            <span className="preset-bar">
              <span />
            </span>
          )}
          {look === 'graph' && (
            <svg viewBox="0 0 60 20" width="60" height="20">
              <path d="M0,15 L8,13 L15,14 L22,9 L29,11 L36,6 L43,8 L50,4 L60,7" className="preset-line" />
            </svg>
          )}
          {look === 'text' && <Type size={14} strokeWidth={2} />}
          {look === 'level' && (
            <span className="preset-level">
              <Volume2 size={13} strokeWidth={2.4} />
              <span className="preset-bar">
                <span />
              </span>
            </span>
          )}
          {look === 'now-playing' && (
            <span className="preset-np">
              <span className="preset-np-art" />
              <Play {...solid} />
              <Pause {...solid} />
            </span>
          )}
        </span>
      )}
      <span className="preset-label" dir="auto">{presetLabel(preset.name)}</span>
    </span>
  );
}

/** A Preset tile that adds itself to the Layout when clicked. `needs` names the Firmware a Device needs to draw it. */
export function PresetButton({ preset, streams, onAdd, needs = null }: { preset: PresetTile; streams: readonly DataStream[]; onAdd(preset: PresetTile): void; needs?: string | null }) {
  return (
    <Tip label="Add to Layout" side="top">
      <button
        type="button"
        className="preset-button"
        aria-label={needs ? `Add ${presetLabel(preset.name)} to Layout. ${needs}` : `Add ${presetLabel(preset.name)} to Layout`}
        onClick={() => onAdd(preset)}
        data-testid="preset"
        data-preset={preset.id}
      >
        <PresetPreview preset={preset} streams={streams} needs={needs} />
      </button>
    </Tip>
  );
}
