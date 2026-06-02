/**
 * Handle the OAuth callback using the client.init() pattern.
 * Call this on app mount — the library auto-detects the callback URL.
 */
import { initOAuth, handleOAuthCallback, getSession, getOAuthSession } from './auth';
import { setSession } from './store.svelte';
import { setupState, clearSetupState } from './setup-store.svelte';
import { browser } from '$app/environment';

export async function processOAuthCallback(): Promise<boolean> {
  try {
    // First try the init() approach — it auto-detects callbacks
    const result = await initOAuth();
    if (result) {
      // initOAuth saves to localStorage and _currentSession, but we
      // need to update the Svelte store so the UI reacts immediately
      const session = getSession();
      if (session) setSession(session);

      // Check if we're in the setup flow
      if (browser) {
        const raw = localStorage.getItem('arbiter-manager-setup-state');
        if (raw) {
          try {
            const state = JSON.parse(raw);
            if (state.step === 'oauth') {
              // We're in setup flow — update setup state and redirect to /setup
              setupState.patch({
                oauthDid: result.did,
                oauthHandle: result.handle,
                pdsEndpoint: session?.pdsUrl ?? null,
                step: 'app-password',
                error: null,
                loading: false,
              });
              // Navigate to setup page
              const { goto } = await import('$app/navigation');
              goto('/setup', { replaceState: true });
              return true;
            }
          } catch {
            // Ignore parse errors
          }
        }
      }
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

    // Check setup flow again
    if (browser) {
      const raw = localStorage.getItem('arbiter-manager-setup-state');
      if (raw) {
        try {
          const state = JSON.parse(raw);
          if (state.step === 'oauth') {
            setupState.patch({
              oauthDid: session.did,
              oauthHandle: session.handle,
              pdsEndpoint: session.pdsUrl,
              step: 'app-password',
              error: null,
              loading: false,
            });
          }
        } catch {
          // Ignore
        }
      }
    }

    return true;
  } catch (e) {
    console.error('OAuth callback failed:', e);
    // Mark setup as errored if in setup flow
    if (browser) {
      const raw = localStorage.getItem('arbiter-manager-setup-state');
      if (raw) {
        setupState.setError(`OAuth sign-in failed: ${e}`);
      }
    }
    window.history.replaceState(null, '', '/');
    return false;
  }
}