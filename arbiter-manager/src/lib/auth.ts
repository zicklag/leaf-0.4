import { BrowserOAuthClient } from '@atproto/oauth-client-browser';
import { browser } from '$app/environment';
import type { Did } from './types';

/**
 * ATProto OAuth client wrapped for the arbiter manager.
 *
 * For dev (localhost), uses the loopback client pattern:
 *   `clientMetadata: undefined` tells the library to use the hardcoded
 *   loopback metadata. Only works on 127.0.0.1 or [::1] origins.
 *
 * For production, set PUBLIC_OAUTH_CLIENT_ID and PUBLIC_OAUTH_REDIRECT_URI
 * env vars, or update the defaults to your deployed URL.
 */

export interface AuthSession {
  did: Did;
  handle: string;
  displayName?: string;
  avatar?: string;
  pdsUrl: string;
  accessJwt: string;
}

const SESSION_KEY = 'arbiter-manager-auth-session';

let _client: BrowserOAuthClient | null = null;
let _currentSession: AuthSession | null = null;

function isLoopbackOrigin(): boolean {
  if (!browser) return false;
  return (
    location.hostname === '127.0.0.1' ||
    location.hostname === '[::1]' ||
    location.hostname === 'localhost'
  );
}

function getClient(): BrowserOAuthClient {
  if (!_client) {
    // Production: use full client metadata
    // Dev (loopback): pass undefined to use hardcoded loopback metadata
    const clientId =
      import.meta.env.PUBLIC_OAUTH_CLIENT_ID ||
      (isLoopbackOrigin()
        ? undefined
        : 'https://your-app.com/oauth-client-metadata.json');

    _client = new BrowserOAuthClient({
      handleResolver: 'https://resolve.handle.net/.well-known/atproto-did',
      clientMetadata: clientId as any,
    });
  }
  return _client;
}

/** Initialize the OAuth client and process any pending callback. */
export async function initOAuth(): Promise<{ did: Did; handle: string } | null> {
  const client = getClient();
  const result = await client.init();

  if (!result) return null;

  const { session } = result;
  const pdsUrl = await resolvePdsUrl(session.sub);

  const authSession: AuthSession = {
    did: session.sub,
    handle: (session as any).handle || session.sub,
    pdsUrl,
    accessJwt: (session as any).accessJwt || '',
  };

  saveSession(authSession);
  return { did: session.sub, handle: (session as any).handle || session.sub };
}

/** Try to restore a saved session from localStorage. */
export function restoreSession(): AuthSession | null {
  if (!browser) return null;
  try {
    const raw = localStorage.getItem(SESSION_KEY);
    if (!raw) return null;
    const session = JSON.parse(raw) as AuthSession;
    _currentSession = session;
    return session;
  } catch {
    localStorage.removeItem(SESSION_KEY);
    return null;
  }
}

/** Save a session to localStorage. */
function saveSession(session: AuthSession) {
  localStorage.setItem(SESSION_KEY, JSON.stringify(session));
  _currentSession = session;
}

/** Clear the saved session. */
export function clearSession() {
  localStorage.removeItem(SESSION_KEY);
  _currentSession = null;
}

/** Get the current session (restored or active). */
export function getSession(): AuthSession | null {
  return _currentSession;
}

/**
 * Start the OAuth login flow.
 * Prompts the user for their ATProto handle and redirects to their PDS.
 */
export async function login(): Promise<void> {
  const client = getClient();
  // Prompt the user for their handle
  const handle = prompt('Enter your ATProto handle (e.g. user.bsky.social):');
  if (!handle || !handle.trim()) return;

  await client.signIn(handle.trim(), {});
  // The above redirects the browser — code below won't run
}

/**
 * Handle the OAuth callback after the user returns from their PDS.
 * Used as fallback if init() doesn't auto-handle the callback.
 */
export async function handleOAuthCallback(callbackUrl: string): Promise<AuthSession> {
  const client = getClient();
  const result = await client.callback(callbackUrl);

  const pdsUrl = await resolvePdsUrl(result.sub);

  const session: AuthSession = {
    did: result.sub,
    handle: (result as any).handle || result.sub,
    pdsUrl,
    accessJwt: (result as any).accessJwt || '',
  };

  saveSession(session);
  return session;
}

/** Resolve a PDS URL from a DID. */
async function resolvePdsUrl(did: string): Promise<string> {
  const res = await fetch(`https://plc.directory/${did}`);
  if (!res.ok) throw new Error(`Failed to resolve DID: ${did}`);
  const didDoc = await res.json();
  const services = didDoc.service as Array<Record<string, unknown>> | undefined;
  if (services) {
    const pds = services.find(
      (s) => s.id === '#atproto_pds' || s.type === 'AtprotoPersonalDataServer',
    );
    if (pds?.serviceEndpoint) return String(pds.serviceEndpoint);
  }
  throw new Error('No PDS endpoint found in DID document');
}
