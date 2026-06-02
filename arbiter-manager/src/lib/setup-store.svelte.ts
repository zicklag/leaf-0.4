/**
 * Setup wizard persistent state.
 *
 * Survives page navigation (OAuth redirects) via localStorage.
 * Step transitions are manual — no reactive auto-advance.
 *
 * Uses Svelte 5 runes ($state, $derived, $effect) for reactivity.
 */

import { browser } from '$app/environment';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type SetupStep =
  | 'intro'
  | 'oauth'
  | 'app-password'
  | 'email-code'
  | 'select-admin'
  | 'complete';

export interface SetupStateSnapshot {
  step: SetupStep;
  oauthDid: string | null;
  oauthHandle: string | null;
  pdsEndpoint: string | null;
  appPassword: string | null;
  accountDid: string | null;
  emailCode: string | null;
  signedOperation: unknown;
  adminDid: string | null;
  error: string | null;
  loading: boolean;
}

const STORAGE_KEY = 'arbiter-manager-setup-state';

const initial: SetupStateSnapshot = {
  step: 'intro',
  oauthDid: null,
  oauthHandle: null,
  pdsEndpoint: null,
  appPassword: null,
  accountDid: null,
  emailCode: null,
  signedOperation: null,
  adminDid: null,
  error: null,
  loading: false,
};

// ---------------------------------------------------------------------------
// Persistence helpers
// ---------------------------------------------------------------------------

function load(): SetupStateSnapshot {
  if (!browser) return { ...initial };
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...initial };
    const parsed = JSON.parse(raw);
    // Merge with initial to catch any missing fields from old saves
    return { ...initial, ...parsed, error: null, loading: false };
  } catch {
    return { ...initial };
  }
}

function save(snapshot: SetupStateSnapshot) {
  if (!browser) return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshot));
}

export function clearSetupState() {
  if (!browser) return;
  localStorage.removeItem(STORAGE_KEY);
}

// ---------------------------------------------------------------------------
// Reactive state class (Svelte 5 runes)
//
// All fields are $state — mutate them directly and any consuming component
// or $derived expression will react.
// ---------------------------------------------------------------------------

export class SetupState {
  step: SetupStep = $state<SetupStep>('intro');
  /** DID returned from OAuth login */
  oauthDid: string | null = $state(null);
  /** Handle used for OAuth login */
  oauthHandle: string | null = $state(null);
  /** User's PDS endpoint (resolved after OAuth) */
  pdsEndpoint: string | null = $state(null);
  /** App password created by the user on their PDS */
  appPassword: string | null = $state(null);
  /** DID of the account that created the app password */
  accountDid: string | null = $state(null);
  /** Email confirmation token entered by the user */
  emailCode: string | null = $state(null);
  /** Signed PLC operation returned by the PDS */
  signedOperation: unknown = $state(null);
  /** Admin DID selected by the user */
  adminDid: string | null = $state(null);
  /** Error message for the current step */
  error: string | null = $state(null);
  /** Whether the current step is loading */
  loading: boolean = $state(false);

  constructor() {
    const saved = load();
    Object.assign(this, saved);
  }

  /** Go to a step (clears error + loading). */
  goTo(step: SetupStep) {
    this.step = step;
    this.error = null;
    this.loading = false;
  }

  /** Set a single field value. */
  setField<K extends keyof SetupStateSnapshot>(key: K, value: SetupStateSnapshot[K]) {
    (this as any)[key] = value;
  }

  /** Patch multiple fields at once. */
  patch(partial: Partial<SetupStateSnapshot>) {
    Object.assign(this, partial);
  }

  /** Set loading state. */
  setLoading(loading: boolean) {
    this.loading = loading;
  }

  /** Set error message and clear loading. */
  setError(error: string | null) {
    this.error = error;
    this.loading = false;
  }

  /** Reset everything back to initial state. */
  reset() {
    clearSetupState();
    Object.assign(this, initial);
  }
}

/** Singleton reactive setup wizard state. */
export const setupState = new SetupState();

// Auto-persist on every change
$effect.root(() => {
  $effect(() => {
    save({
      step: setupState.step,
      oauthDid: setupState.oauthDid,
      oauthHandle: setupState.oauthHandle,
      pdsEndpoint: setupState.pdsEndpoint,
      appPassword: setupState.appPassword,
      accountDid: setupState.accountDid,
      emailCode: setupState.emailCode,
      signedOperation: setupState.signedOperation,
      adminDid: setupState.adminDid,
      error: setupState.error,
      loading: setupState.loading,
    });
  });
});
