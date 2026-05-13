<script lang="ts">
  import type { ArbiterView } from '../lib/types';
  import SpaceNode from './SpaceNode.svelte';
  import { app } from '../lib/simulation-store.svelte';

  interface Props {
    arbiter: ArbiterView;
    isSelected: boolean;
    selectedSpace: string | null;
  }

  let { arbiter, isSelected, selectedSpace }: Props = $props();
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions a11y_click_events_have_key_events -->
<div
  class="arbiter-node"
  class:selected={isSelected}
  onclick={() => app.selectArbiter(arbiter.did)}
  role="region"
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
  </div>

  <div class="spaces-list">
    {#each arbiter.spaces as space}
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
</style>
