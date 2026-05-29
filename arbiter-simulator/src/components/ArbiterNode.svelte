<script lang="ts">
  import type { ArbiterSnapshot } from '../lib/types';
  import SpaceNode from './SpaceNode.svelte';
  import { app } from '../lib/simulation-store.svelte';

  interface Props {
    arbiter: ArbiterSnapshot;
    isSelected: boolean;
    selectedSpace: string | null;
  }

  let { arbiter, isSelected, selectedSpace }: Props = $props();

  let offline = $derived(app.isArbiterOffline(arbiter.did));
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="arbiter-node"
  class:selected={isSelected}
  class:offline
  onclick={() => app.selectArbiter(arbiter.did)}
  onkeydown={(e) => e.key === 'Enter' && app.selectArbiter(arbiter.did)}
  role="button"
  tabindex="0"
  aria-label={`Arbiter ${arbiter.did}`}
  data-arbiter-did={arbiter.did}
>
  <div class="arbiter-header">
    <span class="arbiter-icon">🏛️</span>
    <div class="arbiter-info">
      <span class="arbiter-did mono truncate">{arbiter.did}</span>
      <span class="arbiter-version">v{arbiter.version}</span>
    </div>
    <span class="space-count">{arbiter.spaces.length} space{arbiter.spaces.length !== 1 ? 's' : ''}</span>
    <button
      class="power-toggle"
      class:offline
      onclick={(e) => { e.stopPropagation(); app.toggleArbiterOffline(arbiter.did); }}
      onkeydown={(e) => { e.stopPropagation(); if (e.key === 'Enter') app.toggleArbiterOffline(arbiter.did); }}
      title={offline ? 'Bring arbiter online' : 'Take arbiter offline'}
      aria-label={offline ? 'Bring arbiter online' : 'Take arbiter offline'}
    >
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 2v8"/>
        <path d="M18.36 4.64a9 9 0 1 1-12.73 0"/>
      </svg>
    </button>
  </div>

  <div class="spaces-list">
    {#each [...arbiter.spaces].sort((a, b) => a.key === '$admin' ? -1 : b.key === '$admin' ? 1 : 0) as space}
      <SpaceNode
        {space}
        arbiterDid={arbiter.did}
        isSelected={space.key === selectedSpace}
      />
    {/each}
  </div>
</div>

<style>
  .arbiter-node {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    min-width: 280px;
    max-width: 400px;
    cursor: pointer;
    transition: all 200ms var(--ease-out);
    box-shadow: var(--shadow-sm);
  }

  .arbiter-node:hover {
    box-shadow: var(--shadow-md);
  }

  .arbiter-node.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px oklch(0.58 0.18 65 / 0.2);
  }

  .arbiter-node.offline {
    opacity: 0.55;
    border-color: var(--border-light);
  }

  .arbiter-node.offline.selected {
    border-color: var(--text-muted);
    box-shadow: 0 0 0 2px var(--border);
  }

  .arbiter-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border-light);
  }

  .arbiter-icon {
    font-size: 1.143rem;
  }

  .arbiter-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .arbiter-did {
    font-size: 0.857rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .arbiter-node.offline .arbiter-did {
    color: var(--text-muted);
  }

  .arbiter-version {
    font-size: 0.714rem;
    color: var(--text-muted);
  }

  .space-count {
    font-size: 0.714rem;
    color: var(--text-muted);
    background: var(--bg-base);
    padding: 2px 8px;
    border-radius: var(--radius-xs);
  }

  .spaces-list {
    display: flex;
    flex-direction: column;
    padding: 8px;
    gap: 4px;
  }

  .power-toggle {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-xs);
    background: var(--bg-base);
    cursor: pointer;
    color: var(--accent);
    transition: all 150ms var(--ease-out);
    flex-shrink: 0;
  }

  .power-toggle:hover {
    background: var(--accent-subtle);
    border-color: var(--accent);
  }

  .power-toggle.offline {
    color: oklch(0.55 0.18 25);
    opacity: 1;
    border-color: oklch(0.55 0.18 25 / 0.4);
    background: oklch(0.55 0.18 25 / 0.08);
  }

  .power-toggle.offline:hover {
    color: oklch(0.6 0.2 145);
    border-color: oklch(0.55 0.15 145 / 0.5);
    background: oklch(0.55 0.15 145 / 0.12);
  }
</style>
