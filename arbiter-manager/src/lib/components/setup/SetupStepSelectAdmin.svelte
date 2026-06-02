<script lang="ts">
  import { Button, Input } from '@foxui/core';
  import { setupState } from '$lib/setup-store.svelte';
  import { resolveToDid, resolveDidDocument, getArbiterServiceUrl } from '$lib/pds-client';
  import { addManagedCommunity } from '$lib/store.svelte';

  let adminInput = $state('');
  let loading = $derived(setupState.loading);
  let error = $derived(setupState.error);
  let adminResolved = $state<string | null>(null);
  let adminResolving = $state(false);
  let setupComplete = $state(false);

  function goBack() {
    setupState.goTo('email-code');
  }

  async function resolveAdmin() {
    const value = adminInput.trim();
    if (!value) {
      error = 'Please enter a handle or DID';
      return;
    }

    adminResolving = true;
    error = null;
    adminResolved = null;

    try {
      const did = await resolveToDid(value);
      adminResolved = did;
    } catch (e) {
      error = `Could not resolve "${value}": ${e instanceof Error ? e.message : String(e)}`;
    } finally {
      adminResolving = false;
    }
  }

  async function finishSetup() {
    if (!adminResolved) {
      error = 'Please resolve an admin DID first';
      return;
    }

    loading = true;
    error = null;
    setupState.setLoading(true);

    try {
      const raw = localStorage.getItem('arbiter-manager-setup-state');
      const state = raw ? JSON.parse(raw) : {};

      if (!state.oauthDid) {
        throw new Error('Missing setup state. Please start over.');
      }

      // Verify that the DID document now has the arbiter service
      const didDoc = await resolveDidDocument(state.oauthDid);
      const arbiterUrl = getArbiterServiceUrl(didDoc);

      if (!arbiterUrl) {
        error =
          'Could not find the #arbiter service in your DID document. ' +
          'The PLC operation may not have propagated yet. Try again in a few seconds.';
        loading = false;
        setupState.setLoading(false);
        return;
      }

      // Store the admin selection
      setupState.patch({
        adminDid: adminResolved,
        step: 'complete',
        error: null,
        loading: false,
      });
    } catch (e) {
      error = `Failed: ${e instanceof Error ? e.message : String(e)}`;
      loading = false;
      setupState.setLoading(false);
    }
  }
</script>

<div class="max-w-lg mx-auto px-6 py-12 space-y-6">
  <div class="space-y-2">
    <h2 class="text-xl font-semibold text-base-900 dark:text-base-50">Select an Admin</h2>
    <p class="text-sm text-base-600 dark:text-base-400">
      Choose someone to have <strong>Owner</strong> access to this community's arbiter.
      This person will be able to manage spaces, members, and policies on behalf of
      the community account.
    </p>
  </div>

  <div class="space-y-4">
    <div class="flex flex-col gap-2">
      <label for="admin-input" class="text-sm font-medium text-base-700 dark:text-base-300">
        Admin Handle or DID
      </label>
      <div class="flex gap-2">
        <Input
          id="admin-input"
          bind:value={adminInput}
          placeholder="admin.bsky.social"
          disabled={loading || adminResolving}
          class="flex-1"
        />
        <Button
          variant="secondary"
          onclick={resolveAdmin}
          disabled={loading || adminResolving || !adminInput.trim()}
        >
          {adminResolving ? '…' : 'Lookup'}
        </Button>
      </div>
    </div>

    {#if adminResolved}
      <div class="p-3 rounded-lg bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800">
        <p class="text-sm text-green-700 dark:text-green-300">
          Resolved: <code class="font-mono text-xs bg-green-100 dark:bg-green-800 px-1 rounded">{adminResolved}</code>
        </p>
      </div>
    {/if}

    {#if error}
      <p class="text-sm text-red-500">{error}</p>
    {/if}
  </div>

  <div class="flex justify-between pt-2">
    <Button variant="ghost" onclick={goBack} disabled={loading}>
      Back
    </Button>
    <Button
      onclick={finishSetup}
      disabled={loading || !adminResolved}
    >
      {loading ? 'Finalizing…' : 'Complete Setup'}
    </Button>
  </div>
</div>