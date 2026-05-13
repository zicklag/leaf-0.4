<script lang="ts">
  import { app } from './lib/simulation-store.svelte';
  import Toolbar from './components/Toolbar.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import Canvas from './components/Canvas.svelte';
  import DetailPanel from './components/DetailPanel.svelte';
  import Notifications from './components/Notifications.svelte';
  import EmptyState from './components/EmptyState.svelte';

  let { loading, initError } = $derived(app);

  $effect(() => {
    app.init();
  });
</script>

{#if loading}
  <div class="loading">
    <div class="spinner"></div>
    <p>Loading Arbiter Engine…</p>
  </div>
{:else if initError}
  <div class="error-state">
    <h2>Failed to initialize</h2>
    <p class="mono">{initError}</p>
  </div>
{:else}
  <Toolbar />
  <div class="main-layout">
    <Sidebar />
    {#if app.serverState && app.serverState.arbiters.length > 0}
      <Canvas />
    {:else}
      <div class="canvas-area">
        <EmptyState />
      </div>
    {/if}
    {#if app.selectedSpace}
      <DetailPanel />
    {/if}
  </div>
  <Notifications />
{/if}

<style>
  .loading {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    height: 100vh;
    background: var(--bg-base);
    color: var(--text-secondary);
  }

  .spinner {
    width: 28px;
    height: 28px;
    border: 3px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .error-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    height: 100vh;
    background: var(--bg-base);
    padding: 32px;
    text-align: center;
  }

  .error-state h2 {
    color: oklch(0.45 0.15 20);
  }

  .main-layout {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  .canvas-area {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }
</style>
