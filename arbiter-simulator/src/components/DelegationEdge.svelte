<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

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

    // Top-middle of source space
    x1 = fromRect.left + fromRect.width / 2 - canvasRect.left;
    y1 = fromRect.top - canvasRect.top;
    // Top-middle of target space
    x2 = toRect.left + toRect.width / 2 - canvasRect.left;
    y2 = toRect.top - canvasRect.top;
    visible = true;
  }

  let cleanup: () => void;

  onMount(() => {
    updatePositions();
    const observer = new ResizeObserver(updatePositions);
    const canvas = document.querySelector('.canvas');
    if (canvas) {
      observer.observe(canvas);
      const scrollEl = canvas.querySelector('.canvas-scroll');
      if (scrollEl) {
        scrollEl.addEventListener('scroll', updatePositions);
      }
    }

    cleanup = () => {
      observer.disconnect();
    };
  });

  onDestroy(() => {
    cleanup?.();
  });

  // Compute quadratic bezier control point above the midpoint
  function bezierPath(): string {
    const mx = (x1 + x2) / 2;
    const my = Math.min(y1, y2) - 60; // control point above both nodes
    return `M ${x1} ${y1} Q ${mx} ${my} ${x2} ${y2}`;
  }

  // Compute arrowhead at the target (pointing in from top)
  function arrowPoints(): string {
    const dx = x2 - x1;
    const dy = y2 - y1;
    const angle = Math.atan2(dy, dx);
    const size = 8;
    const xa = x2 - size * Math.cos(angle - Math.PI / 6);
    const ya = y2 - size * Math.sin(angle - Math.PI / 6);
    const xb = x2 - size * Math.cos(angle + Math.PI / 6);
    const yb = y2 - size * Math.sin(angle + Math.PI / 6);
    return `${x2},${y2} ${xa},${ya} ${xb},${yb}`;
  }
</script>

{#if visible}
  <g>
    <!-- Glow line behind for depth -->
    <path
      d={bezierPath()}
      class="edge-glow"
    />
    <!-- Main stroke -->
    <path
      d={bezierPath()}
      class="edge"
    />
    <!-- Arrowhead -->
    <polygon
      class="arrow"
      points={arrowPoints()}
    />
    <!-- Access label at midpoint -->
    <text
      x={(x1 + x2) / 2}
      y={Math.min(y1, y2) - 68}
      class="edge-label"
      text-anchor="middle"
    >
      {access}
    </text>
  </g>
{/if}

<style>
  .edge-glow {
    fill: none;
    stroke: var(--accent);
    stroke-width: 6;
    opacity: 0.08;
    stroke-linecap: round;
  }

  .edge {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
    opacity: 0.65;
    stroke-linecap: round;
  }

  .arrow {
    fill: var(--accent);
    opacity: 0.65;
  }

  .edge-label {
    font-size: 10px;
    font-family: var(--font-mono);
    fill: var(--accent);
    opacity: 0.85;
    font-weight: 500;
  }
</style>
