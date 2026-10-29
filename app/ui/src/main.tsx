import '@fontsource-variable/archivo/wdth.css';
import './styles/tokens.css';
import './styles/app.css';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App.tsx';
import { CompanionContext, connect, EventsContext, pageEnvironment, type Environment } from './companion/connection.ts';
import { createUpdatesStore, UpdatesContext, watchForUpdates } from './state/updates.ts';

// The demo and dev branches are dropped from production builds, fake Companion and all.
const env: Environment =
  import.meta.env.MODE === 'demo'
    ? (await import('./companion/demo.ts')).demoEnvironment(window)
    : import.meta.env.DEV
      ? (await import('./companion/dev.ts')).devEnvironment(window)
      : pageEnvironment(window);

const { companion, events } = connect(env);
const updates = createUpdatesStore(env.updates);
watchForUpdates(updates);

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <CompanionContext value={companion}>
      <EventsContext value={events}>
        <UpdatesContext value={updates}>
          <App />
        </UpdatesContext>
      </EventsContext>
    </CompanionContext>
  </StrictMode>,
);
