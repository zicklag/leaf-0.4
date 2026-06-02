<script lang="ts">
  import { Button, Input } from '@foxui/core';
  import { setupState, setupLoading, setupError } from '$lib/setup-store.svelte';
  import { getOAuthSession } from '$lib/auth';
  import { PdsSetupClient } from '$lib/pds-client';

  let emailCode = $state('');
  let loading = $state(false);
  let error = $state<string | null>(null);
  let codeSent = $state(false);
  let stepLabel = $state('Request confirmation code');

  setupLoading.subscribe((v) => (loading = v));
  setupError.subscribe((v) => (error = v));

  function goBack() {
    setupState.goTo('app-password');
  }

  /**
   * Build the services map for the PLC operation.
   * Preserves the existing atproto_pds service and adds the arbiter service.
   */
  async function buildServicesMap(): Promise<
    Record<string, { type: string; endpoint: string }>
  > {
    const raw = localStorage.getItem('arbiter-manager-setup-state');
    const state = raw ? JSON.parse(raw) : {};

    if (!state.pdsEndpoint || !state.oauthDid) {
      throw new Error('Missing setup state (PDS endpoint or DID)');
    }

    // Fetch the current DID document
    const didDocRes = await fetch(
      `https://plc.directory/${encodeURIComponent(state.oauthDid)}`,
    );
    if (!didDocRes.ok) {
      throw new Error(`Failed to resolve current DID document`);
    }
    const didDoc = await didDocRes.json();

    // Get existing services from DID document
    const existingServices: Record<string, { type: string; endpoint: string }> = {};
    const services = didDoc.service as Array<Record<string, unknown>> | undefined;
    if (services) {
      for (const svc of services) {
        const id = (svc.id as string)?.replace(/^did[^#]*#/, '#');
        if (id && svc.type && svc.serviceEndpoint) {
          existingServices[id] = {
            type: svc.type as string,
            endpoint: svc.serviceEndpoint as string,
          };
        }
      }
    }

    // Ensure atproto_pds is present
    if (!existingServices['#atproto_pds']) {
      existingServices['#atproto_pds'] = {
        type: 'AtprotoPersonalDataServer',
        endpoint: state.pdsEndpoint,
      };
    }

    // Get the arbiter server URL from the public env or config
    const arbiterEndpoint =
      (typeof import.meta !== 'undefined' &&
        (import.meta as any).env?.PUBLIC_ARBITER_SERVER_URL) ||
      ''; // Fallback — configurable

    // Add the arbiter service
    existingServices['#arbiter'] = {
      type: 'MuniTownArbiter',
      endpoint: arbiterEndpoint || `${window.location.origin}/api/arbiter`,
    };

    return existingServices;
  }

  async function requestCode() {
    loading = true;
    error = null;
    stepLabel = 'Requesting code…';
    setupState.setLoading(true);

    try {
      const raw = localStorage.getItem('arbiter-manager-setup-state');
      const state = raw ? JSON.parse(raw) : {};

      if (!state.oauthDid || !state.pdsEndpoint) {
        throw new Error('Missing setup state. Please start over.');
      }

      const oauthSession = await getOAuthSession(state.oauthDid);
      if (!oauthSession) {
        throw new Error('OAuth session expired. Please sign in again.');
      }

      const client = new PdsSetupClient(oauthSession, state.pdsEndpoint);

      // Request the email confirmation code
      await client.requestPlcOperationSignature();

      codeSent = true;
      stepLabel = 'Enter the code';
      loading = false;
      setupState.setLoading(false);
    } catch (e) {
      error = `Failed to request code: ${e instanceof Error ? e.message : String(e)}`;
      loading = false;
      setupState.setLoading(false);
    }
  }

  async function verifyCode() {
    const code = emailCode.trim();
    if (!code) {
      error = 'Please enter the confirmation code';
      return;
    }

    loading = true;
    error = null;
    stepLabel = 'Verifying & signing…';
    setupState.setLoading(true);

    try {
      const raw = localStorage.getItem('arbiter-manager-setup-state');
      const state = raw ? JSON.parse(raw) : {};

      if (!state.oauthDid || !state.pdsEndpoint) {
        throw new Error('Missing setup state. Please start over.');
      }

      const oauthSession = await getOAuthSession(state.oauthDid);
      if (!oauthSession) {
        throw new Error('OAuth session expired. Please sign in again.');
      }

      const client = new PdsSetupClient(oauthSession, state.pdsEndpoint);

      // Build the services map
      const services = await buildServicesMap();

      // Sign the PLC operation
      const signedOperation = await client.signPlcOperation(code, services);

      // Submit the PLC operation
      await client.submitPlcOperation(signedOperation);

      // Save the signed operation (for reference) and move forward
      setupState.patch({
        emailCode: code,
        signedOperation,
        step: 'select-admin',
        error: null,
        loading: false,
      });
    } catch (e) {
      error = `PLC operation failed: ${e instanceof Error ? e.message : String(e)}`;
      loading = false;
      setupState.setLoading(false);
    }
  }
</script>

<div class="max-w-lg mx-auto px-6 py-12 space-y-6">
  <div class="space-y-2">
    <h2 class="text-xl font-semibold text-base-900 dark:text-base-50">Email Confirmation</h2>
    <p class="text-sm text-base-600 dark:text-base-400">
      Your PDS will send a confirmation code to your email. This is required to authorize
      changes to your DID document that add the arbiter service entry.
    </p>
  </div>

  <!-- Info card -->
  <div class="p-4 rounded-lg bg-base-50 dark:bg-base-900 border border-base-200 dark:border-base-800 space-y-1 text-sm">
    <p class="text-base-700 dark:text-base-300">
      <strong>What's happening:</strong> We're adding a <code class="text-xs bg-base-200 dark:bg-base-700 px-1 rounded">#arbiter</code>
      service of type <code class="text-xs bg-base-200 dark:bg-base-700 px-1 rounded">MuniTownArbiter</code> to your DID document.
      Your existing services (<code class="text-xs bg-base-200 dark:bg-base-700 px-1 rounded">#atproto_pds</code>) are preserved.
    </p>
  </div>

  {#if !codeSent}
    <!-- Step 1: Request the code -->
    <div class="space-y-4">
      <Button
        onclick={requestCode}
        disabled={loading}
        class="w-full"
      >
        {loading ? 'Sending request…' : 'Send Confirmation Code'}
      </Button>
      {#if error}
        <p class="text-sm text-red-500">{error}</p>
      {/if}
    </div>

    <div class="flex justify-between pt-2">
      <Button variant="ghost" onclick={goBack} disabled={loading}>
        Back
      </Button>
    </div>

  {:else}
    <!-- Step 2: Enter the code -->
    <div class="space-y-4">
      <div class="p-3 rounded-lg bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800">
        <p class="text-sm text-green-700 dark:text-green-300">
          Confirmation code sent! Check your email for a message from your PDS.
        </p>
      </div>

      <div class="flex flex-col gap-2">
        <label for="code-input" class="text-sm font-medium text-base-700 dark:text-base-300">
          Confirmation Code
        </label>
        <Input
          id="code-input"
          bind:value={emailCode}
          placeholder="000000"
          maxlength={6}
          disabled={loading}
          class="text-center text-lg tracking-widest"
        />
        <p class="text-xs text-base-500">
          Enter the 6-digit code from the email.
        </p>
      </div>

      {#if error}
        <p class="text-sm text-red-500">{error}</p>
      {/if}
    </div>

    <div class="flex justify-between pt-2">
      <Button variant="ghost" onclick={requestCode} disabled={loading}>
        Resend Code
      </Button>
      <Button onclick={verifyCode} disabled={loading || emailCode.trim().length < 6}>
        {loading ? 'Signing & Submitting…' : 'Verify & Continue'}
      </Button>
    </div>
  {/if}
</div>