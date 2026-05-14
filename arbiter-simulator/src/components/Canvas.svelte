<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';
  import ArbiterNode from './ArbiterNode.svelte';
  import DelegationEdge from './DelegationEdge.svelte';

  let { serverState, selectedArbiterDid, selectedSpaceKey } = $derived(app);

  let delegateEdges = $derived.by(() => {
    const ss = serverState;
    const arbDid = selectedArbiterDid;
    const spKey = selectedSpaceKey;
    if (!ss || !arbDid || !spKey) return [];

    const arbiter = ss.arbiters.find(a => a.did === arbDid);
    if (!arbiter) return [];
    const space = arbiter.spaces.find(s => s.key === spKey);
    if (!space) return [];

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
            fromArbiter: arbDid,
            fromSpace: spKey,
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
