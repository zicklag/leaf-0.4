/**
 * Setup wizard persistent state.
 *
 * Survives page navigation (OAuth redirects) via localStorage.
 * Step transitions are manual — no reactive auto-advance.
 */

import { writable, derived } from 'svelte/store';
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

export interface SetupState {
  step: SetupStep;
  /** DID returned from OAuth login */
  oauthDid: string | null;
  /** Handle used for OAuth login */
  oauthHandle: string | null;
  /** User's PDS endpoint (resolved after OAuth) */
  pdsEndpoint: string | null;
  /** App password created by the user on their PDS */
  appPassword: string | null;
  /** DID of the account that created the app password */
  accountDid: string | null;
  /** Email confirmation token entered by the user */
  emailCode: string | null;
  /** Signed PLC operation returned by the PDS */
  signedOperation: unknown;
  /** Admin DID selected by the user */
  adminDid: string | null;
  /** Error message for the current step */
  error: string | null;
  /** Whether the current step is loading */
  loading: boolean;
}

const STORAGE_KEY = 'arbiter-manager-setup-state';

const initial: SetupState = {
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

function load(): SetupState {
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

function save(state: SetupState) {
  if (!browser) return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
}

export function clearSetupState() {
  if (!browser) return;
  localStorage.removeItem(STORAGE_KEY);
}

// ---------------------------------------------------------------------------
// Svelte store
// ---------------------------------------------------------------------------

function createSetupStore() {
  const stored = load();
  const { subscribe, set, update } = writable<SetupState>(stored);

  // Auto-persist on every change
  subscribe((val) => save(val));

  return {
    subscribe,
    /** Go to a step (clears error + loading) */
    goTo(step: SetupStep) {
      update((s) => ({ ...s, step, error: null, loading: false }));
    },
    /** Set a single field value */
    setField<K extends keyof SetupState>(key: K, value: SetupState[K]) {
      update((s) => ({ ...s, [key]: value }));
    },
    /** Patch multiple fields at once */
    patch(partial: Partial<SetupState>) {
      update((s) => ({ ...s, ...partial }));
    },
    /** Set loading state */
    setLoading(loading: boolean) {
      update((s) => ({ ...s, loading }));
    },
    /** Set error message */
    setError(error: string | null) {
      update((s) => ({ ...s, error, loading: false }));
    },
    /** Reset everything back to initial state */
    reset() {
      clearSetupState();
      set({ ...initial });
    },
  };
}

export const setupState = createSetupStore();

// Derived helpers
export const setupStep = derived(setupState, ($s) => $s.step);
export const setupLoading = derived(setupState, ($s) => $s.loading);
export const setupError = derived(setupState, ($s) => $s.error);