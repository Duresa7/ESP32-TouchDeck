import type { ComponentType } from 'react';
import { Activity, Braces, Cpu, Gauge, Music2, Plug, Server, Type, Wifi, type LucideProps } from 'lucide-react';
import type { DataStream, PresetTile } from '../../companion/api.ts';
import { presetLabel, presetLook } from '../../lib/integrations/model.ts';
import { Tip } from '../layout/Tip.tsx';
import '../../styles/integrations.css';

const GLYPHS: Partial<Record<string, ComponentType<LucideProps>>> = {
  activity: Activity,
  braces: Braces,
  cpu: Cpu,
  gauge: Gauge,
  music: Music2,
  server: Server,
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

function PresetPreview({ preset, streams }: { preset: PresetTile; streams: readonly DataStream[] }) {
  const look = presetLook(preset, streams);
  return (
    <span className="preset" data-look={look}>
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
      </span>
      <span className="preset-label">{presetLabel(preset.name)}</span>
    </span>
  );
}

/** A Preset tile that adds itself to the Layout when clicked. */
export function PresetButton({ preset, streams, onAdd }: { preset: PresetTile; streams: readonly DataStream[]; onAdd(preset: PresetTile): void }) {
  return (
    <Tip label="Add to Layout" side="top">
      <button
        type="button"
        className="preset-button"
        aria-label={`Add ${presetLabel(preset.name)} to Layout`}
        onClick={() => onAdd(preset)}
        data-testid="preset"
        data-preset={preset.id}
      >
        <PresetPreview preset={preset} streams={streams} />
      </button>
    </Tip>
  );
}
