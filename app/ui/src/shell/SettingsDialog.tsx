import { useEffect, useId, useRef, useState } from 'react';
import { Dialog } from '@base-ui/react/dialog';
import { Toggle } from '@base-ui/react/toggle';
import { ToggleGroup } from '@base-ui/react/toggle-group';
import { ChevronRight, X } from 'lucide-react';
import type { Appearance } from '../state/theme.ts';
import { useUi, type SettingsSection } from '../state/ui.ts';
import { button } from './button.ts';
import { About } from './settings/About.tsx';
import { BrandfetchEntry, BrandfetchView } from './settings/Brandfetch.tsx';
import { PairedBrowsers } from './settings/PairedBrowsers.tsx';
import { StartAtSignIn } from './settings/StartAtSignIn.tsx';
import { TroubleshootingView } from './settings/Troubleshooting.tsx';

const appearances: { value: Appearance; label: string }[] = [
  { value: 'system', label: 'Match Windows' },
  { value: 'light', label: 'Light' },
  { value: 'dark', label: 'Dark' },
];

function GeneralView({ onTroubleshooting, onBrandfetch, focusEntry }: { onTroubleshooting(): void; onBrandfetch(): void; focusEntry: SettingsSection | null }) {
  const appearance = useUi((s) => s.appearance);
  const setAppearance = useUi((s) => s.setAppearance);
  const entry = useRef<HTMLButtonElement>(null);
  const ids = useId();
  useEffect(() => {
    if (focusEntry === 'troubleshooting') entry.current?.focus();
  }, [focusEntry]);

  return (
    <>
      <div className="dialog-head">
        <Dialog.Title className="dialog-title">Settings</Dialog.Title>
        <Dialog.Close className={button({ kind: 'icon' })} aria-label="Close">
          <X size={18} strokeWidth={1.9} aria-hidden />
        </Dialog.Close>
      </div>

      <section className="settings-section">
        <div className="group">
          <StartAtSignIn />
          <div className="row">
            <span className="row-text">
              <span className="row-title" id={`${ids}-appearance`}>
                Appearance
              </span>
            </span>
            <ToggleGroup
              value={[appearance]}
              onValueChange={(value) => {
                const next = appearances.find((a) => a.value === value[0]);
                if (next) setAppearance(next.value);
              }}
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

      <PairedBrowsers />

      <BrandfetchEntry onOpen={onBrandfetch} focus={focusEntry === 'brandfetch'} />

      <section className="settings-section">
        <div className="group">
          <button ref={entry} type="button" className="row row-button" onClick={onTroubleshooting}>
            <span className="row-text">
              <span className="row-title">Troubleshooting</span>
              <span className="row-sub">Device details, and tools to check that taps reach this PC.</span>
            </span>
            <ChevronRight className="row-chevron" size={18} strokeWidth={1.9} aria-hidden />
          </button>
        </div>
      </section>

      <section className="settings-section" aria-labelledby={`${ids}-about`}>
        <h3 id={`${ids}-about`}>About</h3>
        <div className="group">
          <About />
        </div>
      </section>
    </>
  );
}

function SettingsViews() {
  const section = useUi((s) => s.settingsSection);
  const show = useUi((s) => s.showSettingsSection);
  // The view the person came back from, so focus lands on its entry row again.
  const [from, setFrom] = useState<SettingsSection | null>(null);
  const go = (next: SettingsSection) => {
    setFrom(section);
    show(next);
  };
  if (section === 'troubleshooting') return <TroubleshootingView onBack={() => go('general')} focusBack={from !== null} />;
  if (section === 'brandfetch') return <BrandfetchView onBack={() => go('general')} focusBack={from !== null} />;
  return <GeneralView onTroubleshooting={() => go('troubleshooting')} onBrandfetch={() => go('brandfetch')} focusEntry={from} />;
}

export function SettingsDialog() {
  const popup = useRef<HTMLDivElement>(null);

  return (
    <Dialog.Portal>
      <Dialog.Backdrop className="scrim" />
      <Dialog.Popup
        ref={popup}
        className="dialog dialog-settings"
        // Keyboard users land on the first control; pointer users get no stray focus ring.
        initialFocus={(type) => (type === 'keyboard' ? true : popup.current)}
      >
        <SettingsViews />
      </Dialog.Popup>
    </Dialog.Portal>
  );
}
