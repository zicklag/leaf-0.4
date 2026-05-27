/**
 * Handle the OAuth callback using the client.init() pattern.
 * Call this on app mount — the library auto-detects the callback URL.
 */
import { initOAuth, handleOAuthCallback } from './auth';
import { setSession } from './store.svelte';

export async function processOAuthCallback(): Promise<boolean> {
  try {
    // First try the init() approach — it auto-detects callbacks
    const result = await initOAuth();
    if (result) {
      return true;
    }

    // Fallback: check if this is a callback URL manually
    const url = new URL(window.location.href);
    const hasCode = url.searchParams.has('code');
    const hasError = url.searchParams.has('error');
    const isCallbackPath = url.pathname === '/oauth/callback';

    if (!hasCode && !hasError) return false;
    if (!isCallbackPath && !hasCode) return false;

    if (hasError) {
      const error = url.searchParams.get('error');
      const description = url.searchParams.get('error_description');
      console.error('OAuth error:', description || error);
      window.history.replaceState(null, '', '/');
      return false;
    }

    const session = await handleOAuthCallback(window.location.href);
    window.history.replaceState(null, '', '/');
    setSession(session);
    return true;
  } catch (e) {
    console.error('OAuth callback failed:', e);
    window.history.replaceState(null, '', '/');
    return false;
  }
}
