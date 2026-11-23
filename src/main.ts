import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';

/**
 * Desktop development: the window opens Vite's page with the engine's
 * one-time token in the address; hand it to the engine (through Vite's proxy)
 * so it sets the session cookie, then drop it from the address bar. In
 * production the engine serves the page itself and does this on the first load.
 */
async function handshake(): Promise<void> {
  const url = new URL(location.href);
  const token = url.searchParams.get('token');
  if (!token) return;
  try {
    await fetch(`/api/session?token=${encodeURIComponent(token)}`, { credentials: 'same-origin' });
  } catch {
    /* the app shows its own errors */
  }
  url.searchParams.delete('token');
  history.replaceState(null, '', url.pathname + url.search + url.hash);
}

const app = handshake().then(() => mount(App, { target: document.getElementById('app')! }));

export default app;
