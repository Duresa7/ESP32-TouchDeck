import { useLayoutEffect } from 'react';
import { MotionConfig } from 'motion/react';
import { Toaster } from 'sonner';
import { Home } from './screens/Home.tsx';
import { Integrations } from './screens/Integrations.tsx';
import { Layout } from './screens/Layout.tsx';
import { NavCluster } from './shell/NavCluster.tsx';
import { applyTheme } from './state/theme.ts';
import { useUi } from './state/ui.ts';

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
      {section === 'layout' && <Layout />}
      {section === 'integrations' && <Integrations />}
      <NavCluster />
      <Toaster theme={appearance} position="bottom-right" />
    </MotionConfig>
  );
}
