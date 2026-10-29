import type { ComponentType } from 'react';
import {
  Activity,
  AppWindow,
  ArrowBigUp,
  FolderOpen,
  Keyboard,
  Music2,
  PanelsTopLeft,
  Pause,
  Play,
  SkipBack,
  SkipForward,
  SquareTerminal,
  Type,
  Volume1,
  Volume2,
  VolumeX,
  type LucideProps,
} from 'lucide-react';
import type { ActionType } from '../../lib/editor/validate.ts';

export const KINDS: Record<ActionType, { name: string; line: string; noun: string; icon: ComponentType<LucideProps> }> = {
  key: { name: 'Keyboard shortcut', line: 'Press keys on this PC', noun: 'Keyboard shortcut', icon: Keyboard },
  launch: { name: 'Open an app', line: 'Start a program', noun: 'App', icon: AppWindow },
  consumer: { name: 'Media control', line: 'Play, pause, skip', noun: 'Media control', icon: Music2 },
  page: { name: 'Go to a Page', line: 'Switch Pages on this Device', noun: 'Page jump', icon: PanelsTopLeft },
  mod: { name: 'Hold a modifier', line: 'Ctrl, Shift, Alt or Windows', noun: 'Modifier key', icon: ArrowBigUp },
  open: { name: 'Open a file', line: 'Or a folder or website', noun: 'File or website', icon: FolderOpen },
  command: { name: 'Run a command', line: 'Asks before it runs', noun: 'Command', icon: SquareTerminal },
  none: { name: 'Label only', line: 'Does nothing when tapped', noun: 'Label', icon: Type },
  widget: { name: 'Live tile', line: 'Shows a live value', noun: 'Live tile', icon: Activity },
};

const solid = (size: number) => ({ size, strokeWidth: 0, fill: 'currentColor' });
const line = (size: number) => ({ size, strokeWidth: 2.2 });

export function BuiltinIcon({ icon, size }: { icon: string; size: number }) {
  switch (icon) {
    case 'lv:prev':
      return <SkipBack {...solid(size)} aria-hidden />;
    case 'lv:next':
      return <SkipForward {...solid(size)} aria-hidden />;
    case 'lv:play_pause':
      return (
        <span className="glyph-pair" aria-hidden>
          <Play {...solid(size * 0.8)} />
          <Pause {...solid(size * 0.8)} />
        </span>
      );
    case 'lv:mute':
      return <VolumeX {...line(size)} aria-hidden />;
    case 'lv:volume_mid':
      return <Volume1 {...line(size)} aria-hidden />;
    case 'lv:volume_max':
      return <Volume2 {...line(size)} aria-hidden />;
    default:
      return null;
  }
}

const BUILTIN: ReadonlySet<string> = new Set(['lv:prev', 'lv:next', 'lv:play_pause', 'lv:mute', 'lv:volume_mid', 'lv:volume_max']);

export function TileGlyph({ icon, kind, size }: { icon: string | null; kind: ActionType; size: number }) {
  if (icon && BUILTIN.has(icon)) return <BuiltinIcon icon={icon} size={size} />;
  const Kind = KINDS[kind].icon;
  return <Kind size={size} strokeWidth={1.9} aria-hidden />;
}
