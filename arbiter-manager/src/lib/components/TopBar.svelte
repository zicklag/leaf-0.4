<script lang="ts">
  import { Button, ThemeToggle } from '@foxui/core';
  import { session, login, logout, isAuthenticated } from '$lib/store.svelte';

  let authenticated = $state(false);
  let currentSession = $state(session);

  isAuthenticated.subscribe((v) => (authenticated = v));
  session.subscribe((s) => (currentSession = s));
</script>

<header
  class="flex items-center justify-between px-4 py-2.5 border-b border-base-200 dark:border-base-800 bg-base-50 dark:bg-base-950 shrink-0"
>
  <div class="flex items-center gap-3">
    <span class="text-lg font-semibold tracking-tight text-base-900 dark:text-base-50">
      Arbiter Manager
    </span>
  </div>

  <div class="flex items-center gap-2">
    {#if authenticated && currentSession}
      <div class="flex items-center gap-2 text-sm text-base-600 dark:text-base-400 mr-2">
        <span class="hidden sm:inline truncate max-w-40">{currentSession.handle}</span>
      </div>

      <ThemeToggle />
      <Button variant="ghost" size="sm" onclick={logout}>Sign Out</Button>
    {:else}
      <ThemeToggle />
      <Button onclick={login}>Sign in with ATProto</Button>
    {/if}
  </div>
</header>
