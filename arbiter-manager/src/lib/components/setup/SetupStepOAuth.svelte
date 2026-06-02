<script lang="ts">
  import { Button, Input } from '@foxui/core';
  import { setupState } from '$lib/setup-store.svelte';
  import { auth } from '$lib/auth.svelte';

  let handleValue = $state('');
  let loading = $derived(setupState.loading);
  let error = $derived(setupState.error);

  async function proceedWithLogin() {
    const handle = handleValue.trim();
    if (!handle) {
      error = 'Please enter your handle';
      return;
    }

    setupState.setField('oauthHandle', handle);
    setupState.setLoading(true);

    try {
      // Save setup state before redirect
      setupState.patch({
        step: 'oauth',
        error: null,
        loading: true,
      });

      await auth.loginWithHandle(handle);
      // After the above returns, the browser has redirected
      // (or will redirect). If we get here, something went wrong.
      setupState.setError('OAuth login failed to initiate. Please try again.');
    } catch (e) {
      setupState.setError(`Login failed: ${e}`);
    }
  }

  function goBack() {
    setupState.goTo('intro');
  }
</script>

<div class="max-w-lg mx-auto px-6 py-12 space-y-6">
  <div class="space-y-2">
    <h2 class="text-xl font-semibold text-base-900 dark:text-base-50">Sign In with AT Protocol</h2>
    <p class="text-sm text-base-600 dark:text-base-400">
      Sign in with the account that owns the community DID. You'll be redirected to your PDS to
      authorize this app via OAuth.
    </p>
  </div>

  <div class="space-y-4">
    <div class="flex flex-col gap-2">
      <label for="handle-input" class="text-sm font-medium text-base-700 dark:text-base-300">
        Your AT Protocol Handle or DID
      </label>
      <Input
        id="handle-input"
        bind:value={handleValue}
        placeholder="account.bsky.social or did:plc:abc123"
        disabled={loading}
      />
      <p class="text-xs text-base-500">
        This is the account whose DID document will be updated with the arbiter service.
      </p>
    </div>

    {#if error}
      <p class="text-sm text-red-500">{error}</p>
    {/if}
  </div>

  <div class="flex justify-between pt-2">
    <Button variant="ghost" onclick={goBack} disabled={loading}>Back</Button>
    <Button onclick={proceedWithLogin} disabled={loading || !handleValue.trim()}>
      {loading ? 'Redirecting to PDS…' : 'Sign In with OAuth'}
    </Button>
  </div>
</div>
