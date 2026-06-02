<script lang="ts">
  import { onMount } from 'svelte';
  import { browser } from '$app/environment';
  import { goto } from '$app/navigation';
  import { setupState, setupStep, clearSetupState } from '$lib/setup-store.svelte';
  import SetupStepIntro from '$lib/components/setup/SetupStepIntro.svelte';
  import SetupStepOAuth from '$lib/components/setup/SetupStepOAuth.svelte';
  import SetupStepAppPassword from '$lib/components/setup/SetupStepAppPassword.svelte';
  import SetupStepEmailCode from '$lib/components/setup/SetupStepEmailCode.svelte';
  import SetupStepSelectAdmin from '$lib/components/setup/SetupStepSelectAdmin.svelte';
  import SetupStepComplete from '$lib/components/setup/SetupStepComplete.svelte';

  let step = $state<string>('intro');
  let showFullReset = $state(false);

  setupStep.subscribe((v) => (step = v));

  // After OAuth callback lands us here, proceed to app-password step
  // The oauth callback handler should have set the step to 'app-password'
  // if the OAuth login was part of the setup flow.
  onMount(() => {
    // If there's no setup state, treat this as a fresh start
    if (!browser) return;
    const raw = localStorage.getItem('arbiter-manager-setup-state');
    if (!raw) {
      setupState.reset();
    }
  });
</script>

<div class="min-h-full flex flex-col">
  <!-- Top bar -->
  <header class="flex items-center justify-between px-6 py-3 border-b border-base-200 dark:border-base-800">
    <div class="flex items-center gap-3">
      <a href="/" class="text-sm text-accent-500 hover:text-accent-600 font-medium">
        &larr; Home
      </a>
      <h1 class="text-lg font-semibold text-base-900 dark:text-base-50">
        Setup Community Account
      </h1>
    </div>
    <div class="flex items-center gap-2">
      {#if step !== 'intro' && step !== 'complete'}
        <button
          onclick={() => { showFullReset = true; }}
          class="text-xs text-base-500 hover:text-red-500 transition-colors"
        >
          Cancel &amp; Reset
        </button>
      {/if}
    </div>
  </header>

  <!-- Steps indicator -->
  <div class="px-6 py-4 border-b border-base-100 dark:border-base-800">
    <div class="max-w-3xl mx-auto flex items-center gap-2 text-xs font-medium">
      {#each ['intro', 'oauth', 'app-password', 'email-code', 'select-admin', 'complete'] as s, i}
        {@const active = step === s}
        {@const done = ['intro', 'oauth', 'app-password', 'email-code', 'select-admin', 'complete'].indexOf(step) > i}
        <div class="flex items-center gap-2">
          <span
            class="w-6 h-6 rounded-full flex items-center justify-center transition-colors
              {active ? 'bg-accent-500 text-white' : done ? 'bg-green-500 text-white' : 'bg-base-200 dark:bg-base-700 text-base-500'}"
          >
            {#if done}
              <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M5 13l4 4L19 7" />
              </svg>
            {:else}
              {i + 1}
            {/if}
          </span>
          <span class="hidden sm:inline {active ? 'text-base-900 dark:text-base-50 font-semibold' : 'text-base-500'}">
            {s === 'intro' ? 'Overview' : s === 'oauth' ? 'Sign In' : s === 'app-password' ? 'App Password' : s === 'email-code' ? 'Email Code' : s === 'select-admin' ? 'Admin' : 'Done'}
          </span>
          {#if i < 5}
            <span class="w-6 h-px bg-base-300 dark:bg-base-700"></span>
          {/if}
        </div>
      {/each}
    </div>
  </div>

  <!-- Step content -->
  <div class="flex-1 overflow-auto">
    {#if step === 'intro'}
      <SetupStepIntro />
    {:else if step === 'oauth'}
      <SetupStepOAuth />
    {:else if step === 'app-password'}
      <SetupStepAppPassword />
    {:else if step === 'email-code'}
      <SetupStepEmailCode />
    {:else if step === 'select-admin'}
      <SetupStepSelectAdmin />
    {:else if step === 'complete'}
      <SetupStepComplete />
    {/if}
  </div>
</div>

<!-- Full reset confirmation modal -->
{#if showFullReset}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onclick={() => (showFullReset = false)}>
    <div class="bg-white dark:bg-base-900 rounded-lg p-6 max-w-sm mx-4 shadow-xl" onclick={(e) => e.stopPropagation()}>
      <h3 class="text-lg font-semibold text-base-900 dark:text-base-50 mb-2">Reset Setup?</h3>
      <p class="text-sm text-base-600 dark:text-base-400 mb-4">
        This will clear all progress and take you back to the home page.
      </p>
      <div class="flex justify-end gap-2">
        <button
          onclick={() => (showFullReset = false)}
          class="px-4 py-2 text-sm font-medium rounded-lg bg-base-100 dark:bg-base-800 text-base-700 dark:text-base-300 hover:bg-base-200 dark:hover:bg-base-700 transition-colors"
        >
          Keep Going
        </button>
        <button
          onclick={async () => {
            clearSetupState();
            showFullReset = false;
            goto('/');
          }}
          class="px-4 py-2 text-sm font-medium rounded-lg bg-red-500 text-white hover:bg-red-600 transition-colors"
        >
          Reset
        </button>
      </div>
    </div>
  </div>
{/if}