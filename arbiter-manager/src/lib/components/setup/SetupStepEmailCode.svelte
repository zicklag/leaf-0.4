<script lang="ts">
  import { Alert, Button, Input } from '@foxui/core';
  import {
    ARBITER_SERVICE_KEY,
    ARBITER_SERVICE_TYPE,
    buildServicesMap,
    needsServiceUpdate,
    setupState,
  } from '$lib/setupState.svelte';
  import { auth } from '$lib/auth.svelte';
  import { isAtprotoDid } from '@atproto/oauth-client-browser';

  let emailCode = $state('');
  let codeSent = $state(false);

  let needsUpdate = $derived(
    isAtprotoDid(auth.did) ? needsServiceUpdate(auth.did) : new Promise(() => {}),
  );

  function goBack() {
    setupState.step = 'app-password';
  }

  async function requestCode() {
    setupState.loading = true;
    setupState.error = undefined;
    setupState.loading = true;

    try {
      if (!setupState.appPassword || !auth.agent)
        throw new Error('Missing app password or session');

      await auth.agent.com.atproto.identity.requestPlcOperationSignature();

      codeSent = true;
      setupState.loading = false;
    } catch (e) {
      setupState.error = `Failed to request code: ${e instanceof Error ? e.message : String(e)}`;
      setupState.loading = false;
    }
  }

  async function verifyCode() {
    const code = emailCode.trim();

    setupState.loading = true;
    setupState.error = undefined;

    try {
      if (!auth.agent || !isAtprotoDid(auth.did)) throw new Error('Not logged in');

      // Build the services map
      const services = await buildServicesMap();

      if (await needsServiceUpdate(auth.did)) {
        // Sign the PLC operation
        const resp = await auth.agent.com.atproto.identity.signPlcOperation({
          services,
          token: code,
        });
        const operation = resp.data.operation;

        // Submit the PLC operation
        await auth.agent.com.atproto.identity.submitPlcOperation({
          operation,
        });
      }

      // Save the signed operation (for reference) and move forward
      setupState.step = 'select-admin';
      setupState.error = undefined;
      setupState.loading = false;
    } catch (e) {
      setupState.error = `PLC operation failed: ${e instanceof Error ? e.message : String(e)}`;
      setupState.loading = false;
    }
  }
</script>

{#await needsUpdate then needsUpdate}
  {#if needsUpdate}
    <div class="max-w-lg mx-auto px-6 py-12 space-y-6">
      <div class="space-y-2">
        <h2 class="text-xl font-semibold text-base-900 dark:text-base-50">Email Confirmation</h2>
        <p class="text-sm text-base-600 dark:text-base-400">
          Your PDS will send a confirmation code to your email. This is required to authorize
          changes to your DID document that add the arbiter service entry.
        </p>
      </div>

      <!-- Info card -->
      <div
        class="p-4 rounded-lg bg-base-50 dark:bg-base-900 border border-base-200 dark:border-base-800 space-y-1 text-sm"
      >
        <p class="text-base-700 dark:text-base-300">
          <strong>What's happening:</strong> We're adding a service endpoint to your DID with a
          <code class="text-xs bg-base-200 dark:bg-base-700 px-1 rounded"
            >#{ARBITER_SERVICE_KEY}</code
          >
          key and the
          <code class="text-xs bg-base-200 dark:bg-base-700 px-1 rounded"
            >{ARBITER_SERVICE_TYPE}</code
          >
          type.
          <br /><br />This will tell compatible apps which arbiter to use to communicate with this
          account.
        </p>
      </div>

      {#if !codeSent}
        <!-- Step 1: Request the code -->
        <div class="space-y-4 align-center flex flex-col">
          <Button onclick={requestCode} disabled={setupState.loading} class="w-full">
            {setupState.loading ? 'Sending request…' : 'Send Confirmation Code'}
          </Button>
          <button
            class="text-accent-400 text-center text-sm w-full"
            onclick={() => (codeSent = true)}
          >
            Already Have a Code
          </button>

          {#if setupState.error}
            <p class="text-sm text-red-500">{setupState.error}</p>
          {/if}
        </div>

        <div class="flex justify-between pt-2">
          <Button variant="ghost" onclick={goBack} disabled={setupState.loading}>Back</Button>
        </div>
      {:else}
        <!-- Step 2: Enter the code -->
        <div class="space-y-4">
          <div
            class="p-3 rounded-lg bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800"
          >
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
              disabled={setupState.loading}
              class="text-center text-lg tracking-widest"
            />
            <p class="text-xs text-base-500">Enter the code from the email.</p>
          </div>

          {#if setupState.error}
            <p class="text-sm text-red-500">{setupState.error}</p>
          {/if}
        </div>

        <div class="flex justify-between pt-2">
          <Button variant="ghost" onclick={requestCode} disabled={setupState.loading}
            >Resend Code</Button
          >
          <Button onclick={verifyCode} disabled={setupState.loading || emailCode.trim().length < 6}>
            {setupState.loading ? 'Signing & Submitting…' : 'Verify & Continue'}
          </Button>
        </div>
      {/if}
    </div>
  {:else}
    <div class="max-w-lg mx-auto px-6 py-12 space-y-6">
      <Alert type="info" class="text-md max-w-xl">
        <span>The arbiter service has already been configured properly for this account!</span>
      </Alert>
      <div class="max-w-lg flex items-center justify-between">
        <Button variant="ghost" onclick={goBack} disabled={setupState.loading}>Back</Button>
        <Button onclick={() => (setupState.step = 'select-admin')}>Continue</Button>
      </div>
    </div>
  {/if}
{/await}
