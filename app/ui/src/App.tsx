import { lazy, Suspense, useLayoutEffect } from 'react';
import { MotionConfig } from 'motion/react';
import { Toaster } from 'sonner';
import { Home } from './screens/Home.tsx';
import { Integrations } from './screens/Integrations.tsx';
import { NavCluster } from './shell/NavCluster.tsx';
import { applyTheme } from './state/theme.ts';
import { useUi } from './state/ui.ts';

// The Layout editor loads the first time its screen opens, so the shell's own bundle stays small.
const Layout = lazy(() => import('./screens/Layout.tsx').then((screen) => ({ default: screen.Layout })));

/**
 * The app shell: the current screen, the nav cluster over it and the toast
 * outlet. Switching screens never animates. Motion follows the system's reduced
 * motion setting everywhere; the shell's own transitions are CSS and tokens.css
 * handles them.
 */
export function App() {
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
      {section === 'integrations' && <Integrations />}
      <NavCluster />
      <Toaster theme={appearance} position="bottom-right" />
    </MotionConfig>
  );
}
