<script lang="ts">
  import { Button, Input } from '@foxui/core';
  import { setupState } from '$lib/setup-store.svelte';
  import { PdsSetupClient } from '$lib/pds-client';

  let appPassword = $state('');
  let loading = $derived(setupState.loading);
  let error = $derived(setupState.error);

  function goBack() {
    setupState.goTo('oauth');
  }

  async function verifyAndProceed() {
    const pwd = appPassword.trim();
    if (!pwd) {
      error = 'Please enter your App Password';
      return;
    }

    loading = true;
    error = null;
    setupState.setLoading(true);

    try {
      // Get the OAuth session
      const oauthDid = localStorage.getItem('arbiter-manager-setup-state')
        ? JSON.parse(localStorage.getItem('arbiter-manager-setup-state')!).oauthDid
        : null;

      if (!oauthDid) {
        throw new Error('No OAuth session found. Please sign in first.');
      }

      const oauthSession = await getOAuthSession(oauthDid);
      if (!oauthSession) {
        throw new Error('OAuth session expired. Please sign in again.');
      }

      // Resolve PDS URL from DID
      const pdsUrl = localStorage.getItem('arbiter-manager-setup-state')
        ? JSON.parse(localStorage.getItem('arbiter-manager-setup-state')!).pdsEndpoint
        : null;

      if (!pdsUrl) {
        throw new Error('Could not resolve PDS URL. Please sign in again.');
      }

      // Verify the app password works
      const client = new PdsSetupClient(oauthSession, pdsUrl);
      const accountDid = await client.verifyAppPassword(pwd);

      if (!accountDid) {
        error = 'App Password verification failed. Please check the password and try again.';
        loading = false;
        setupState.setLoading(false);
        return;
      }

      // Save the app password
      setupState.patch({
        appPassword: pwd,
        accountDid,
        step: 'email-code',
        error: null,
        loading: false,
      });
    } catch (e) {
      error = `Verification failed: ${e instanceof Error ? e.message : String(e)}`;
      loading = false;
      setupState.setLoading(false);
    }
  }
</script>

<div class="max-w-lg mx-auto px-6 py-12 space-y-6">
  <div class="space-y-2">
    <h2 class="text-xl font-semibold text-base-900 dark:text-base-50">Enter Your App Password</h2>
    <p class="text-sm text-base-600 dark:text-base-400">
      Create an <strong>App Password</strong> on your PDS and enter it below.
      The arbiter server will use this to proxy authorization checks through your PDS
      on your behalf.
    </p>
  </div>

  <!-- Instructions link -->
  <div class="p-4 rounded-lg bg-accent-50 dark:bg-accent-900/20 border border-accent-200 dark:border-accent-800 text-sm space-y-2">
    <p class="font-medium text-accent-800 dark:text-accent-200">How to create an App Password</p>
    <ol class="list-decimal list-inside text-accent-700 dark:text-accent-300 space-y-1 text-xs">
      <li>Go to your PDS settings (or Bluesky Settings &rarr; App Passwords)</li>
      <li>Create a new App Password with a name like "arbiter-manager"</li>
      <li>Copy the generated password and paste it below</li>
    </ol>
  </div>

  <div class="space-y-4">
    <div class="flex flex-col gap-2">
      <label for="app-password-input" class="text-sm font-medium text-base-700 dark:text-base-300">
        App Password
      </label>
      <Input
        id="app-password-input"
        type="password"
        bind:value={appPassword}
        placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
        disabled={loading}
      />
      <p class="text-xs text-base-500">
        The arbiter server stores this securely and uses it to communicate with your PDS.
      </p>
    </div>

    {#if error}
      <p class="text-sm text-red-500">{error}</p>
    {/if}
  </div>

  <div class="flex justify-between pt-2">
    <Button variant="ghost" onclick={goBack} disabled={loading}>
      Back
    </Button>
    <Button onclick={verifyAndProceed} disabled={loading || !appPassword.trim()}>
      {loading ? 'Verifying…' : 'Continue'}
    </Button>
  </div>
</div>