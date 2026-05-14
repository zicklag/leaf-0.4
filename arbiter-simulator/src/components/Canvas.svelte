<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';
  import ArbiterNode from './ArbiterNode.svelte';
  import DelegationEdge from './DelegationEdge.svelte';

  let { serverState, selectedArbiterDid, selectedSpaceKey } = $derived(app);

  // Find the selected space's remote delegations to know which edges to show
  let delegateEdges = $derived.by(() => {
    if (!serverState || !selectedArbiterDid || !selectedSpaceKey) return [];

    // Find the selected space
    const arbiter = serverState.arbiters.find(a => a.did === selectedArbiterDid);
    if (!arbiter) return [];
    const space = arbiter.spaces.find(s => s.key === selectedSpaceKey);
    if (!space) return [];

    // Find all RemoteSpace members in the selected space
    const edges: Array<{
      fromArbiter: string;
      fromSpace: string;
      toArbiter: string;
      toSpace: string;
      access: string;
    }> = [];

    for (const member of space.members) {
      if (member.memberType === 'RemoteSpace') {
        const slashIdx = member.value.indexOf('/');
        if (slashIdx > 0) {
          edges.push({
            fromArbiter: selectedArbiterDid,
            fromSpace: selectedSpaceKey,
            toArbiter: member.value.slice(0, slashIdx),
            toSpace: member.value.slice(slashIdx + 1),
            access: member.access,
          });
        }
      }
    }

    return edges;
  });
</script>

<div class="canvas">
  <div class="canvas-scroll">
    {#if serverState}
      <div class="arbiters-grid">
        {#each serverState.arbiters as arbiter}
          <ArbiterNode
            {arbiter}
            isSelected={arbiter.did === selectedArbiterDid}
            selectedSpace={arbiter.did === selectedArbiterDid ? selectedSpaceKey : null}
          />
        {/each}
      </div>
    {/if}
  </div>

  <!-- SVG overlay for delegation edges — only shown when a space with delegations is selected -->
  {#if delegateEdges.length > 0}
    <svg class="edges-overlay">
      {#each delegateEdges as edge}
        <DelegationEdge
          fromArbiter={edge.fromArbiter}
          fromSpace={edge.fromSpace}
          toArbiter={edge.toArbiter}
          toSpace={edge.toSpace}
          access={edge.access}
        />
      {/each}
    </svg>
  {/if}
</div>

<style>
  .canvas {
    flex: 1;
    position: relative;
    overflow: hidden;
    background: var(--bg-base);
  }

  .canvas-scroll {
    height: 100%;
    overflow: auto;
    padding: 24px;
  }

  .arbiters-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 24px;
    align-items: flex-start;
  }

  .edges-overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
    width: 100%;
    height: 100%;
    z-index: 10;
  }
</style>
