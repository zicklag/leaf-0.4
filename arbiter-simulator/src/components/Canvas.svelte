<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';
  import ArbiterNode from './ArbiterNode.svelte';
  import DelegationEdge from './DelegationEdge.svelte';

  let { serverState, selectedArbiterDid, selectedSpaceKey } = $derived(app);
</script>

<div class="canvas">
  <div class="canvas-scroll">
    {#if serverState}
      <div class="arbiters-grid">
        {#each serverState.arbiters as arbiter}
          <ArbiterNode
            {arbiter}
            isSelected={arbiter.did === selectedArbiterDid}
            selectedSpace={selectedSpaceKey}
          />
        {/each}
      </div>
    {/if}
  </div>

  <!-- SVG overlay for delegation edges -->
  <svg class="edges-overlay">
    {#if serverState}
      {#each serverState.arbiters as arbiter}
        {#each arbiter.spaces as space}
          {#each space.members as member}
            {#if member.memberType === 'RemoteSpace'}
              {@const slashIdx = member.value.indexOf('/')}
              {@const targetArb = member.value.slice(0, slashIdx)}
              {@const targetSpace = member.value.slice(slashIdx + 1)}
              <DelegationEdge
                fromArbiter={arbiter.did}
                fromSpace={space.key}
                toArbiter={targetArb}
                toSpace={targetSpace}
                access={member.access}
              />
            {/if}
          {/each}
        {/each}
      {/each}
    {/if}
  </svg>
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
