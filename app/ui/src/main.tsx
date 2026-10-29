import '@fontsource-variable/archivo/wdth.css';
import './styles/tokens.css';
import './styles/app.css';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { Pair, takePairingCode } from './screens/Pair.tsx';
import { App } from './App.tsx';
import { CompanionContext, connect, pageEnvironment, type Environment } from './companion/connection.ts';

const isPair = window.location.pathname === '/pair';
const initialCode = isPair ? takePairingCode(window.location, window.history) : '';

// The demo and dev branches are dropped from production builds, fake Companion and all.
const env: Environment =
  import.meta.env.MODE === 'demo'
    ? (await import('./companion/demo.ts')).demoEnvironment(window)
    : import.meta.env.DEV
      ? (await import('./companion/dev.ts')).devEnvironment(window)
      : pageEnvironment(window);

const companion = isPair ? null : connect(env).companion;

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <CompanionContext value={companion}>
      {isPair ? <Pair initialCode={initialCode} /> : <App />}
    </CompanionContext>
  </StrictMode>,
);
