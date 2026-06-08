import init, { validate_policy } from 'arbiter-core-wasm';

let ready = false;
let initPromise: Promise<void> | null = null;

/**
 * Ensure the WASM module is initialised (idempotent).
 */
export async function ensureWasm(): Promise<void> {
  if (ready) return;
  if (!initPromise) {
    initPromise = init().then(() => {
      ready = true;
    });
  }
  await initPromise;
}

/**
 * Validate a Rego policy string.
 * Returns `null` if valid, or an error message string if invalid.
 */
export function checkPolicy(policy: string): string | null {
  if (!ready) {
    return 'WASM not initialised yet';
  }
  try {
    validate_policy(policy);
    return null;
  } catch (e) {
    return String(e);
  }
}