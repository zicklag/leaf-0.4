<script lang="ts">
  import type { SpaceSnapshot } from '../lib/types';
  import { app } from '../lib/simulation-store.svelte';
  import { accessLabel, shortDid, parseMemberDid } from '../lib/utils';

  interface Props {
    space: SpaceSnapshot;
    arbiterDid: string;
    isSelected: boolean;
  }

  let { space, arbiterDid, isSelected }: Props = $props();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="space-node"
  class:selected={isSelected}
  class:admin={space.key === '$admin'}
  onclick="{(e) => { e.stopPropagation(); app.selectSpace(arbiterDid, space.key); }}"
  role="button"
  tabindex="0"
  onkeydown={(e) => e.key === 'Enter' && app.selectSpace(arbiterDid, space.key)}
  data-arbiter-did={arbiterDid}
  data-space-key={space.key}
>
  <div class="space-header">
    <span class="space-icon">{space.key === '$admin' ? '👑' : '📁'}</span>
    <span class="space-key">{space.key}</span>
    <div class="space-badges">
      {#if space.config?.publicMembers}
        <span class="badge public" title="Public members">public</span>
      {/if}
      {#if space.config?.publicRecords}
        <span class="badge records" title="Public records">records</span>
      {/if}
    </div>
  </div>

  <div class="space-members-preview">
    {#if space.members.length === 0}
      <span class="no-members">No direct members</span>
    {:else}
      {#each space.members.slice(0, 3) as m}
        {@const info = parseMemberDid(m.did)}
        <span class="member-chip mono" title={`${info.kind}: ${info.display} → ${accessLabel(m.access)}`}>
          {info.kind === 'user' ? '👤' : info.kind === 'remotespace' ? '🌐' : '📁'}
          {shortDid(info.display, 16)}
        </span>
      {/each}
      {#if space.members.length > 3}
        <span class="more-count">+{space.members.length - 3} more</span>
      {/if}
    {/if}
  </div>
</div>

<style>
  .space-node {
    padding: 8px 12px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border-light);
    cursor: pointer;
    transition: all 150ms var(--ease-out);
  }

  .space-node:hover {
    background: var(--accent-subtle);
  }

  .space-node.selected {
    background: var(--accent-subtle);
    border-color: var(--accent);
    box-shadow: 0 0 0 1px oklch(0.58 0.18 65 / 0.15);
  }

  .space-node.admin {
    border-color: var(--accent);
    border-width: 1.5px;
  }

  .space-header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 6px;
  }

  .space-icon {
    font-size: 0.857rem;
  }

  .space-key {
    font-weight: 600;
    font-size: 0.857rem;
  }

  .space-badges {
    display: flex;
    gap: 4px;
    margin-left: auto;
  }

  .badge {
    font-size: 0.643rem;
    padding: 1px 5px;
    border-radius: var(--radius-xs);
    background: var(--border);
    color: var(--text-muted);
    font-weight: 500;
  }

  .badge.public {
    background: oklch(0.92 0.06 65 / 0.6);
    color: var(--accent-text);
  }

  .space-members-preview {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }

  .no-members {
    font-size: 0.714rem;
    color: var(--text-muted);
    font-style: italic;
  }

  .member-chip {
    font-size: 0.714rem;
    padding: 1px 6px;
    border-radius: var(--radius-xs);
    background: var(--bg-base);
    color: var(--text-secondary);
  }

  .more-count {
    font-size: 0.714rem;
    color: var(--text-muted);
  }
</style>
