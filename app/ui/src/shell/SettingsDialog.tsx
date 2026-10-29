import { useEffect, useId, useRef, useState } from 'react';
import { Dialog } from '@base-ui/react/dialog';
import { Toggle } from '@base-ui/react/toggle';
import { ToggleGroup } from '@base-ui/react/toggle-group';
import { X } from 'lucide-react';
import type { SystemInfo } from '../companion/api.ts';
import { useCompanion } from '../companion/connection.ts';
import type { Appearance } from '../state/theme.ts';
import { useUi } from '../state/ui.ts';
import { useCheckForUpdates } from '../state/updates.ts';
import { button } from './button.ts';
import { checkText } from './update-text.ts';

const appearances: { value: Appearance; label: string }[] = [
  { value: 'system', label: 'Match Windows' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
];

function useSystemInfo() {
  const companion = useCompanion();
  const [info, setInfo] = useState<SystemInfo | 'unknown' | null>(null);
  useEffect(() => {
    let live = true;
    companion.system.get().then(
      (system) => live && setInfo(system),
      () => live && setInfo('unknown'),
    );
    return () => {
      live = false;
    };
  }, [companion]);
  return info;
}

function About() {
  const info = useSystemInfo();
  const { supported, check, checkForUpdates } = useCheckForUpdates();
  const version = info === null ? '' : info === 'unknown' ? ' Unknown' : ` ${info.version}`;
  const status = checkText(check);
  return (
    <div className="row">
      <span className="row-text">
        <span className="row-title">Open TouchDeck{version}</span>
        {info !== null && info !== 'unknown' && <span className="row-sub tabular">Build {info.build}</span>}
        <span className="row-sub" aria-live="polite">
          {status}
        </span>
      </span>
      {supported && check !== 'disabled' && (
        <button
          type="button"
          className={button()}
          aria-disabled={check === 'checking' || undefined}
          onClick={() => void checkForUpdates()}
        >
          Check for updates
        </button>
      )}
    </div>
  );
}

/**
 * Settings, a dialog over whatever screen is open. Escape closes it. What it
 * changes lives in the UI store, so it's still there when Settings opens again.
 * The sections are placeholders until #88 fills them.
 */
export function SettingsDialog() {
  const popup = useRef<HTMLDivElement>(null);
  const appearance = useUi((s) => s.appearance);
  const setAppearance = useUi((s) => s.setAppearance);
  const ids = useId();
  const section = useUi((s) => s.settingsSection);

  return (
    <Dialog.Portal>
      <Dialog.Backdrop className="scrim" />
      <Dialog.Popup
        ref={popup}
        className="dialog dialog-settings"
        // Keyboard users land on the first control; pointer users get no stray focus ring.
        initialFocus={(type) => (type === 'keyboard' ? true : popup.current)}
      >
        <div className="dialog-head">
          <Dialog.Title className="dialog-title">Settings</Dialog.Title>
          <Dialog.Close className={button({ kind: 'icon' })} aria-label="Close">
            <X size={18} strokeWidth={1.9} aria-hidden />
          </Dialog.Close>
        </div>

        <section className="settings-section">
          <div className="group">
            <div className="row">
              <span className="row-text">
                <span className="row-title" id={`${ids}-appearance`}>
                  Appearance
                </span>
              </span>
              <ToggleGroup
                value={[appearance]}
                onValueChange={(value) => value[0] && setAppearance(value[0] as Appearance)}
                aria-labelledby={`${ids}-appearance`}
                className="segmented"
              >
                {appearances.map((a) => (
                  <Toggle key={a.value} value={a.value} className="segment">
                    {a.label}
                  </Toggle>
                ))}
              </ToggleGroup>
            </div>
          </div>
        </section>

        <section className="settings-section" aria-labelledby={`${ids}-browsers`}>
          <h3 id={`${ids}-browsers`}>Paired browsers</h3>
          <div className="group">
            <div className="row">
              <span className="row-note">No browsers paired. Choose Open in browser in the tray to pair one.</span>
            </div>
          </div>
        </section>

        {section === 'troubleshooting' && <section className="settings-section" aria-labelledby={`${ids}-troubleshooting`}>
          <h3 id={`${ids}-troubleshooting`}>Troubleshooting</h3>
          <div className="group"><div className="row"><span className="row-note">Device details, and tools to check that taps reach this PC.</span></div></div>
        </section>}
        <section className="settings-section" aria-labelledby={`${ids}-about`}>
          <h3 id={`${ids}-about`}>About</h3>
          <div className="group">
            <About />
          </div>
        </section>
      </Dialog.Popup>
    </Dialog.Portal>
  );
}
