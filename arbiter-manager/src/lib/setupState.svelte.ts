/**
 * Setup wizard persistent state.
 *
 * Survives page navigation (OAuth redirects) via localStorage.
 * Step transitions are manual — no reactive auto-advance.
 *
 * Uses Svelte 5 runes ($state, $derived, $effect) for reactivity.
 */

import { type } from 'arktype';
import { PdsSetupClient } from './pds-setup-client';
import { auth } from './auth.svelte';
import { isAtprotoDid } from '@atproto/oauth-client-browser';
import { didResolver } from './resolver';
import { PUBLIC_ARBITER_URL } from '$env/static/public';
import type { AtprotoDid } from '@atcute/lexicons/syntax';

const STORAGE_KEY = 'arbiter-manager-setup-state';

const setupStepTy = type(
  '"intro" | "oauth" | "app-password" | "email-code" | "select-admin" | "complete"',
);
export type SetupStep = typeof setupStepTy.infer;

const setupStateTy = type({
  step: setupStepTy.default('intro'),
  did: type.string.optional(),
  appPassword: type.string.optional(),
  emailCode: type.string.optional(),
  adminDid: type.string.optional(),
  error: type.string.optional(),
  loading: type.boolean.default(false),
});
export type SetupState = typeof setupStateTy.infer;

const initState: SetupState = {
  step: 'intro',
  loading: false,
};
const loaded = JSON.parse(globalThis.localStorage.getItem(STORAGE_KEY) || '{}');
const parsed = setupStateTy(loaded);

export const setupState: SetupState = $state(parsed instanceof type.errors ? initState : parsed);
export const setupClient = new PdsSetupClient();

export const resetSetupState = () => {
  for (const key in setupState) {
    (setupState as any)[key] = (initState as any)[key];
  }
};

// Auto-persist on every change
$effect.root(() => {
  $effect(() => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ ...$state.snapshot(setupState), error: undefined, loading: false }),
    );
  });
});

/** Get the existing service endpoints for a DID. */
async function getExistingServices(
  did: AtprotoDid,
): Promise<Record<string, { type: string; endpoint: string }>> {
  // Fetch the current DID document
  const didDoc = await didResolver.resolve(did);

  // Get existing services from DID document
  return Object.fromEntries(
    // Get the services
    (didDoc.service || [])
      // Map them to the expected format
      .flatMap((service) => {
        // Make sure the fields are correctly typed
        if (
          !(
            typeof service.id == 'string' &&
            typeof service.type == 'string' &&
            typeof service.serviceEndpoint == 'string'
          )
        ) {
          // Ignore invalid entries
          return [];
        }

        // Return a valid entry
        const entry = [
          service.id.replace('#', ''),
          { type: service.type, endpoint: service.serviceEndpoint },
        ];
        return [entry];
      }),
  );
}

/** Check whether the DID already has the XRPC arbiter service that we need. */
export async function needsServiceUpdate(did: AtprotoDid): Promise<boolean> {
  const existingServices = await getExistingServices(did);
  console.log('existing services', existingServices);
  return (
    existingServices['xrpc_arbiter']?.endpoint != PUBLIC_ARBITER_URL &&
    existingServices['xrpc_arbiter']?.type != 'XrpcArbiter'
  );
}

/**
 * Build the services map for the PLC operation.
 * Preserves the existing atproto_pds services and adds the arbiter service.
 */
export async function buildServicesMap(): Promise<
  Record<string, { type: string; endpoint: string }>
> {
  if (!auth.agent) throw new Error('Not logged in');
  const did = auth.did;
  if (!isAtprotoDid(did)) throw new Error('Invalid DID');

  const services = await getExistingServices(did);

  // Add the arbiter service
  services['xrpc_arbiter'] = {
    type: 'XrpcArbiter',
    endpoint: PUBLIC_ARBITER_URL || `${window.location.origin}`,
  };

  return services;
}
