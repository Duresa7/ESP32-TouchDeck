import { lazy, Suspense, useEffect, useLayoutEffect } from 'react';
import { MotionConfig } from 'motion/react';
import { Toaster, toast } from 'sonner';
import { useCompanion } from './companion/connection.ts';
import { useDevices } from './companion/devices.ts';
import { Home } from './screens/Home.tsx';
import { SetupHost } from './screens/onboarding/SetupHost.tsx';
import { NavCluster } from './shell/NavCluster.tsx';
import { applyTheme } from './state/theme.ts';
import { useUi } from './state/ui.ts';

const Layout = lazy(() => import('./screens/Layout.tsx').then((screen) => ({ default: screen.Layout })));
const Integrations = lazy(() => import('./screens/Integrations.tsx').then((screen) => ({ default: screen.Integrations })));

/**
 * The app shell: the current screen, the nav cluster over it and the toast
 * outlet. Switching screens never animates. Motion follows the system's reduced
 * motion setting everywhere; the shell's own transitions are CSS and tokens.css
 * handles them.
 */
export function App() {
  const companion = useCompanion();
  const status = useDevices(s => s.status);
  useEffect(() => {
    if (status === 'unpaired' && !window.openTouchDeck) location.replace('/pair');
  }, [status]);
  const section = useUi((s) => s.section);
  const appearance = useUi((s) => s.appearance);

  useLayoutEffect(() => applyTheme(document.documentElement, appearance), [appearance]);

  return (
    <MotionConfig reducedMotion="user">
      {section === 'home' && <Home />}
      {section === 'layout' && (
        <Suspense fallback={null}>
          <Layout />
        </Suspense>
      )}
      {section === 'integrations' && (
        <Suspense fallback={null}>
          <Integrations />
        </Suspense>
      )}
      <NavCluster />
      <SetupHost />
      {window.openTouchDeck && <button className="open-browser" type="button" onClick={() => {
        void companion.auth.pair_code().catch(() => toast.error("Can't reach the Companion. Check that Open TouchDeck is running in the tray, then retry."));
      }}>Open in browser</button>}
      <Toaster theme={appearance} position="bottom-right" />
    </MotionConfig>
  );
}
