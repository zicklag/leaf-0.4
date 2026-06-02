<script lang="ts">
  import { Button } from '@foxui/core';
  import { goto } from '$app/navigation';
  import { auth } from '$lib/auth.svelte';
</script>

<section class="flex-1 flex items-center justify-center px-6 py-16">
  <div class="max-w-2xl mx-auto text-center">
    <h1
      class="text-4xl sm:text-5xl font-bold tracking-tight text-base-900 dark:text-base-50 leading-tight"
    >
      Manage Your Community Authorization
    </h1>

    <p class="mt-6 text-lg text-base-600 dark:text-base-400 leading-relaxed max-w-lg mx-auto">
      Complete control over who can publish what under your org account. Granular roles, spaces, and
      policies.
    </p>

    <div class="mt-10 flex flex-col items-center gap-4">
      <Button size="lg" onclick={() => auth.login()}>Sign in with ATProto</Button>
      <p class="text-sm text-base-500 dark:text-base-500">
        Sign in with your Atmosphere account to create or manage your own organization or community
        accounts.
      </p>
    </div>

    <div class="my-6">
      <span class="text-xl text-base-400 dark:text-base-500">or</span>
    </div>

    <div class="flex flex-col items-center gap-4">
      <Button variant="secondary" size="lg" onclick={() => goto('/setup')}>
        Setup Existing Org Account
      </Button>
      <p class="text-sm text-base-500 dark:text-base-500">
        Configure an existing community / organization account so that it can be managed by the
        arbiter.
      </p>
    </div>

    <div class="mt-16 grid grid-cols-1 sm:grid-cols-3 gap-8 text-left">
      <div class="space-y-2">
        <h3 class="font-semibold text-base-900 dark:text-base-50">Org Account Management</h3>
        <p class="text-sm text-base-600 dark:text-base-400 leading-relaxed">
          Create and manage community accounts with full control over identity and authorization.
        </p>
      </div>
      <div class="space-y-2">
        <h3 class="font-semibold text-base-900 dark:text-base-50">
          Customizable Roles &amp; Permissions
        </h3>
        <p class="text-sm text-base-600 dark:text-base-400 leading-relaxed">
          Define granular access for every member. From read-only to full ownership, with spaces and
          delegation chains.
        </p>
      </div>
      <div class="space-y-2">
        <h3 class="font-semibold text-base-900 dark:text-base-50">Publish Control &amp; Beyond</h3>
        <p class="text-sm text-base-600 dark:text-base-400 leading-relaxed">
          Control exactly who can publish what record types under your org. Ready for permissioned
          data when the protocol supports it.
        </p>
      </div>
    </div>
  </div>
</section>
