import { useCheckForUpdates } from '../../state/updates.ts';
import { button } from '../button.ts';
import { checkText } from '../update-text.ts';
import { useSystemInfo } from './system.ts';

export function About() {
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
