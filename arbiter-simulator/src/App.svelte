<script lang="ts">
  import { app } from './lib/simulation-store.svelte';
  import Toolbar from './components/Toolbar.svelte';
  import CreateArbiterBar from './components/CreateArbiterBar.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import Canvas from './components/Canvas.svelte';
  import DetailPanel from './components/DetailPanel.svelte';
  import Notifications from './components/Notifications.svelte';
  import EmptyState from './components/EmptyState.svelte';
  import PolicyEditor from './components/PolicyEditor.svelte';

  type Tab = 'visual' | 'policy';
  let activeTab = $state<Tab>('visual');

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
    <div class="canvas-column">
      <div class="tab-bar">
        <button
          class="tab"
          class:active={activeTab === 'visual'}
          onclick={() => (activeTab = 'visual')}
        >
          <span class="tab-icon">🌐</span>
          <span>Visual</span>
        </button>
        <button
          class="tab"
          class:active={activeTab === 'policy'}
          onclick={() => (activeTab = 'policy')}
        >
          <span class="tab-icon">📝</span>
          <span>Policy</span>
        </button>
      </div>

      {#if activeTab === 'visual'}
        <CreateArbiterBar />
        {#if app.serverState && app.serverState.arbiters.length > 0}
          <Canvas />
        {:else}
          <div class="canvas-area">
            <EmptyState />
          </div>
        {/if}
      {:else}
        <PolicyEditor />
      {/if}
    </div>
    {#if activeTab === 'visual' && app.selectedSpace}
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

  .canvas-column {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .tab-bar {
    display: flex;
    gap: 0;
    background: var(--bg-surface);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    padding: 0 16px;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    border: none;
    border-bottom: 2px solid transparent;
    background: none;
    cursor: pointer;
    font-family: inherit;
    font-size: 0.857rem;
    font-weight: 500;
    color: var(--text-muted);
    transition: all 150ms var(--ease-out);
  }

  .tab:hover {
    color: var(--text-secondary);
    background: var(--accent-subtle);
  }

  .tab.active {
    color: var(--accent-text);
    border-bottom-color: var(--accent);
  }

  .tab-icon {
    font-size: 1rem;
  }
</style>
