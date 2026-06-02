<script lang="ts">
  import { Button } from '@foxui/core';
  import { setupState } from '$lib/setup-store.svelte';

  function startSetup() {
    setupState.goTo('oauth');
  }
</script>

<div class="max-w-3xl mx-auto px-6 py-12 space-y-8">
  <!-- Overview heading -->
  <div class="text-center space-y-3">
    <h2 class="text-2xl font-bold text-base-900 dark:text-base-50">
      Set Up Your Community Account
    </h2>
    <p class="text-base text-base-600 dark:text-base-400 max-w-lg mx-auto">
      A community account is an AT Protocol account that's managed by your organization.
      This wizard will guide you through setting up your PDS, authorizing an arbiter,
      and granting admin access.
    </p>
  </div>

  <!-- Step descriptions -->
  <div class="space-y-4">
    <div class="flex gap-4 p-4 rounded-lg bg-base-50 dark:bg-base-900 border border-base-200 dark:border-base-800">
      <div class="w-8 h-8 rounded-full bg-accent-100 dark:bg-accent-900 text-accent-600 dark:text-accent-300 flex items-center justify-center text-sm font-bold shrink-0">
        1
      </div>
      <div class="space-y-1">
        <h3 class="font-semibold text-base-900 dark:text-base-50">Create an App Password</h3>
        <p class="text-sm text-base-600 dark:text-base-400">
          Go to your AT Protocol PDS (the server that hosts your account) and create an
          <strong>App Password</strong>. The arbiter service uses this long-term to
          proxy authorization checks through your PDS.
        </p>
        <p class="text-xs text-base-500 mt-1">
          <a href="https://atproto.com/guides/app-passwords" target="_blank" rel="noopener noreferrer"
            class="text-accent-500 hover:text-accent-600 underline">
            How to create an AppPassword on your PDS &rarr;
          </a>
        </p>
      </div>
    </div>

    <div class="flex gap-4 p-4 rounded-lg bg-base-50 dark:bg-base-900 border border-base-200 dark:border-base-800">
      <div class="w-8 h-8 rounded-full bg-accent-100 dark:bg-accent-900 text-accent-600 dark:text-accent-300 flex items-center justify-center text-sm font-bold shrink-0">
        2
      </div>
      <div class="space-y-1">
        <h3 class="font-semibold text-base-900 dark:text-base-50">Sign In with AT Protocol (OAuth)</h3>
        <p class="text-sm text-base-600 dark:text-base-400">
          Sign in with your account using AT Protocol OAuth. This allows us to request
          signed PLC operations from your PDS to update your DID document with the
          arbiter service entry.
        </p>
      </div>
    </div>

    <div class="flex gap-4 p-4 rounded-lg bg-base-50 dark:bg-base-900 border border-base-200 dark:border-base-800">
      <div class="w-8 h-8 rounded-full bg-accent-100 dark:bg-accent-900 text-accent-600 dark:text-accent-300 flex items-center justify-center text-sm font-bold shrink-0">
        3
      </div>
      <div class="space-y-1">
        <h3 class="font-semibold text-base-900 dark:text-base-50">Enter App Password</h3>
        <p class="text-sm text-base-600 dark:text-base-400">
          Type in the App Password you created. It will be sent to the arbiter server
          so it can proxy authorization checks through your PDS.
        </p>
      </div>
    </div>

    <div class="flex gap-4 p-4 rounded-lg bg-base-50 dark:bg-base-900 border border-base-200 dark:border-base-800">
      <div class="w-8 h-8 rounded-full bg-accent-100 dark:bg-accent-900 text-accent-600 dark:text-accent-300 flex items-center justify-center text-sm font-bold shrink-0">
        4
      </div>
      <div class="space-y-1">
        <h3 class="font-semibold text-base-900 dark:text-base-50">Verify via Email</h3>
        <p class="text-sm text-base-600 dark:text-base-400">
          Your PDS will send a confirmation code to your email. This is required to
          authorize changes to your DID document (adding the arbiter service).
          Enter the code to proceed.
        </p>
      </div>
    </div>

    <div class="flex gap-4 p-4 rounded-lg bg-base-50 dark:bg-base-900 border border-base-200 dark:border-base-800">
      <div class="w-8 h-8 rounded-full bg-accent-100 dark:bg-accent-900 text-accent-600 dark:text-accent-300 flex items-center justify-center text-sm font-bold shrink-0">
        5
      </div>
      <div class="space-y-1">
        <h3 class="font-semibold text-base-900 dark:text-base-50">Select an Admin</h3>
        <p class="text-sm text-base-600 dark:text-base-400">
          Choose an account that will have <strong>Owner</strong> access to your
          community's arbiter. This person can manage spaces, members, and
          policies on behalf of the community account.
        </p>
      </div>
    </div>
  </div>

  <!-- Start button -->
  <div class="text-center pt-4">
    <Button size="lg" onclick={startSetup}>
      Get Started
    </Button>
    <p class="text-xs text-base-500 mt-3">
      You can cancel at any time. No changes will be made until you confirm.
    </p>
  </div>
</div>