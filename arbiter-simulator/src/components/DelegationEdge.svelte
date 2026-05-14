<script lang="ts">
  import { onMount } from 'svelte';

  interface Props {
    fromArbiter: string;
    fromSpace: string;
    toArbiter: string;
    toSpace: string;
    access: string;
  }

  let { fromArbiter, fromSpace, toArbiter, toSpace, access }: Props = $props();
  let x1 = $state(0);
  let y1 = $state(0);
  let x2 = $state(0);
  let y2 = $state(0);
  let visible = $state(false);

  function updatePositions() {
    const fromEl = document.querySelector(
      `[data-arbiter-did="${fromArbiter}"][data-space-key="${fromSpace}"]`,
    );
    const toEl = document.querySelector(
      `[data-arbiter-did="${toArbiter}"][data-space-key="${toSpace}"]`,
    );

    if (!fromEl || !toEl) {
      visible = false;
      return;
    }

    const canvas = fromEl.closest('.canvas');
    if (!canvas) return;

    const canvasRect = canvas.getBoundingClientRect();
    const fromRect = fromEl.getBoundingClientRect();
    const toRect = toEl.getBoundingClientRect();

    x1 = fromRect.right - canvasRect.left;
    y1 = fromRect.top + fromRect.height / 2 - canvasRect.top;
    x2 = toRect.left - canvasRect.left;
    y2 = toRect.top + toRect.height / 2 - canvasRect.top;
    visible = true;
  }

  onMount(() => {
    updatePositions();
    // Recalculate on resize or scroll
    const observer = new ResizeObserver(updatePositions);
    const canvas = document.querySelector('.canvas');
    if (canvas) {
      observer.observe(canvas);
      // Also observe scroll
      const scrollEl = canvas.querySelector('.canvas-scroll');
      if (scrollEl) {
        scrollEl.addEventListener('scroll', updatePositions);
      }
    }
    return () => {
      observer.disconnect();
    };
  });
</script>

{#if visible}
  <g>
    <line
      class="edge"
      x1={x1} y1={y1} x2={x2} y2={y2}
    />
    <!-- Arrowhead -->
    <polygon
      class="arrow"
      points={arrowPoints(x1, y1, x2, y2)}
    />
  </g>
{/if}

<!-- Logic to compute arrowhead is in the module context -->
<script lang="ts" module>
  function arrowPoints(x1: number, y1: number, x2: number, y2: number): string {
    const angle = Math.atan2(y2 - y1, x2 - x1);
    const size = 8;
    const xa = x2 - size * Math.cos(angle - Math.PI / 6);
    const ya = y2 - size * Math.sin(angle - Math.PI / 6);
    const xb = x2 - size * Math.cos(angle + Math.PI / 6);
    const yb = y2 - size * Math.sin(angle + Math.PI / 6);
    return `${x2},${y2} ${xa},${ya} ${xb},${yb}`;
  }
</script>

<style>
  .edge {
    stroke: var(--accent);
    stroke-width: 2;
    opacity: 0.6;
  }

  .arrow {
    fill: var(--accent);
    opacity: 0.6;
  }
</style>
