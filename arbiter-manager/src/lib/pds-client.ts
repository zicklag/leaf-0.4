/**
 * Client for interacting with an AT Protocol PDS.
 *
 * Uses the OAuth session's fetch handler for auth (DPoP-bound requests)
 * and direct fetch for app-password-based operations.
 */

import type { OAuthSession } from '@atproto/oauth-client-browser';

// ---------------------------------------------------------------------------
// PLC operation type matching the AT Protocol lexicon
// ---------------------------------------------------------------------------

export interface PlcServiceEntry {
  type: string;
  endpoint: string;
}

export interface PlcServices {
  [serviceId: string]: PlcServiceEntry;
}

// ---------------------------------------------------------------------------
// OAuth-backed PDS client (used for PLC operations)
// ---------------------------------------------------------------------------

/**
 * PDS client wrapper for the setup flow.
 * Uses an OAuth session's fetch handler to make signed requests.
 */
export class PdsSetupClient {
  private oauthSession: OAuthSession;
  public pdsUrl: string;

  constructor(oauthSession: OAuthSession, pdsUrl: string) {
    this.pdsUrl = pdsUrl.replace(/\/+$/, '');
    this.oauthSession = oauthSession;
  }

  /**
   * Make a GET request to the PDS through the OAuth session.
   */
  private async get(path: string): Promise<unknown> {
    const url = `${this.pdsUrl}${path}`;
    const res = await this.oauthSession.fetchHandler(url);
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new PdsRequestError(res.status, body.error, body.message);
    }
    return res.json();
  }

  /**
   * Make a POST request to the PDS through the OAuth session.
   */
  private async post(path: string, body: unknown): Promise<unknown> {
    const url = `${this.pdsUrl}${path}`;
    const res = await this.oauthSession.fetchHandler(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      const errorBody = await res.json().catch(() => ({}));
      throw new PdsRequestError(res.status, errorBody.error, errorBody.message);
    }
    const text = await res.text();
    return text ? JSON.parse(text) : undefined;
  }

  // -----------------------------------------------------------------------
  // PLC Operations
  // -----------------------------------------------------------------------

  /**
   * Request an email confirmation token for signing a PLC operation.
   * The user will receive an email with a 6-digit code.
   */
  async requestPlcOperationSignature(): Promise<void> {
    await this.post('/xrpc/com.atproto.identity.requestPlcOperationSignature', {});
  }

  /**
   * Sign a PLC operation that updates the DID document's service entries.
   * This requires the email confirmation token.
   *
   * @param token - The 6-digit email confirmation code
   * @param services - The services map to set in the DID document
   * @returns The signed operation ready to submit
   */
  async signPlcOperation(
    token: string,
    services: PlcServices,
  ): Promise<Record<string, unknown>> {
    const response = (await this.post(
      '/xrpc/com.atproto.identity.signPlcOperation',
      {
        token,
        services,
      },
    )) as { operation: Record<string, unknown> };

    return response.operation;
  }

  /**
   * Submit a signed PLC operation to the PLC directory.
   */
  async submitPlcOperation(operation: unknown): Promise<void> {
    await this.post('/xrpc/com.atproto.identity.submitPlcOperation', {
      operation,
    });
  }

  // -----------------------------------------------------------------------
  // App password helpers
  // -----------------------------------------------------------------------

  /**
   * Verify that an app password works by calling a simple endpoint.
   * Returns the account DID if successful, null otherwise.
   */
  async verifyAppPassword(appPassword: string): Promise<string | null> {
    try {
      // Try to create a session with the app password
      const res = await fetch(`${this.pdsUrl}/xrpc/com.atproto.server.createSession`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          identifier: this.oauthSession.did,
          password: appPassword,
        }),
      });

      if (!res.ok) return null;

      const data = await res.json() as { did?: string };
      return data.did ?? null;
    } catch {
      return null;
    }
  }
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

export class PdsRequestError extends Error {
  status: number;
  errorCode?: string;

  constructor(status: number, errorCode?: string, message?: string) {
    super(message || errorCode || `PDS error ${status}`);
    this.name = 'PdsRequestError';
    this.status = status;
    this.errorCode = errorCode;
  }
}

// ---------------------------------------------------------------------------
// DID resolution helpers
// ---------------------------------------------------------------------------

/**
 * Resolve a DID to get its DID document from plc.directory.
 */
export async function resolveDidDocument(
  did: string,
): Promise<Record<string, unknown>> {
  const res = await fetch(
    `https://plc.directory/${encodeURIComponent(did)}`,
  );
  if (!res.ok) throw new Error(`Failed to resolve DID: ${did}`);
  return res.json();
}

/**
 * Resolve a handle to a DID via the PLC directory.
 */
export async function resolveHandle(handle: string): Promise<string> {
  const res = await fetch(
    `https://plc.directory/${encodeURIComponent(handle)}`,
    { headers: { Accept: 'application/json' } },
  );

  if (res.ok) {
    const doc = await res.json();
    return (doc.id || doc.did || handle) as string;
  }

  // Try via standard AT Protocol handle resolution
  const handleRes = await fetch(
    `https://resolve.handle.net/.well-known/atproto-did?handle=${encodeURIComponent(handle)}`,
  );
  if (handleRes.ok) {
    return handleRes.text();
  }

  throw new Error(`Could not resolve handle: ${handle}`);
}

/**
 * Resolve a handle or DID string to a DID.
 */
export async function resolveToDid(value: string): Promise<string> {
  if (value.startsWith('did:')) return value;
  return resolveHandle(value);
}

/**
 * Resolve the arbiter service endpoint URL from a DID document.
 */
export function getArbiterServiceUrl(
  didDoc: Record<string, unknown>,
): string | null {
  const services = didDoc.service as Array<Record<string, unknown>> | undefined;
  if (!services) return null;
  for (const svc of services) {
    if (svc.id === '#arbiter' || String(svc.id).endsWith('#arbiter')) {
      return (svc.serviceEndpoint as string) ?? null;
    }
    if (svc.type === 'MuniTownArbiter') {
      return (svc.serviceEndpoint as string) ?? null;
    }
  }
  return null;
}

/**
 * Get the PDS endpoint from a DID document.
 */
export function getPdsUrlFromDoc(
  didDoc: Record<string, unknown>,
): string | null {
  const services = didDoc.service as Array<Record<string, unknown>> | undefined;
  if (!services) return null;
  for (const svc of services) {
    if (svc.type === 'AtprotoPersonalDataServer') {
      return (svc.serviceEndpoint as string) ?? null;
    }
  }
  return null;
}