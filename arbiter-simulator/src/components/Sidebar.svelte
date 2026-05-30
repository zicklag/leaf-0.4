<script lang="ts">
  import UserList from './UserList.svelte';
  import ArbiterActions from './ArbiterActions.svelte';
  import AccessLegend from './AccessLegend.svelte';
  import { app } from '../lib/simulation-store.svelte';
</script>

<aside class="sidebar">
  <div class="sidebar-scroll">
    <UserList />
    <ArbiterActions />
    {#if !app.advancedMode}
      <AccessLegend />
    {/if}
  </div>

  <!-- Bottom bar: advanced mode toggle -->
  <div class="bottom-bar">
    <button
      class="advanced-toggle"
      class:active={app.advancedMode}
      onclick={() => app.toggleAdvancedMode()}
      title={app.advancedMode ? 'Switch to simple mode' : 'Switch to advanced mode (JSON editors)'}
    >
      <span class="toggle-icon">{app.advancedMode ? '⚙' : '🔧'}</span>
      <span class="toggle-label">{app.advancedMode ? 'Advanced' : 'Simple'}</span>
      <span class="toggle-switch">
        <span class="toggle-knob" class:right={app.advancedMode}></span>
      </span>
    </button>
  </div>
</aside>

<style>
  .sidebar {
    width: 280px;
    flex-shrink: 0;
    background: var(--bg-surface);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .sidebar-scroll {
    flex: 1;
    overflow-y: auto;
  }

  .bottom-bar {
    flex-shrink: 0;
    padding: 8px 12px;
    border-top: 1px solid var(--border);
    background: var(--bg-base);
  }

  .advanced-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-raised);
    cursor: pointer;
    transition: all 150ms var(--ease-out);
    font-family: inherit;
    font-size: 0.786rem;
    color: var(--text-secondary);
  }

  .advanced-toggle:hover {
    border-color: var(--accent);
    background: var(--accent-subtle);
  }

  .advanced-toggle.active {
    border-color: var(--accent);
    background: oklch(0.58 0.18 65 / 0.08);
  }

  .toggle-icon { font-size: 0.929rem; flex-shrink: 0; }
  .toggle-label { flex: 1; text-align: left; font-weight: 500; }

  .toggle-switch {
    position: relative;
    width: 32px;
    height: 18px;
    border-radius: 9px;
    background: var(--border);
    transition: background 150ms var(--ease-out);
    flex-shrink: 0;
  }

  .advanced-toggle.active .toggle-switch {
    background: var(--accent);
  }

  .toggle-knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: white;
    transition: transform 150ms var(--ease-out);
    box-shadow: 0 1px 2px rgba(0,0,0,0.15);
  }

  .toggle-knob.right {
    transform: translateX(14px);
  }
</style>
