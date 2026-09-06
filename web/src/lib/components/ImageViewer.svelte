<script lang="ts">
  import { RotateCcw, ZoomIn, ZoomOut } from 'lucide-svelte';
  import { rawUrl } from '../api';
  interface Props {
    path: string;
  }

  let { path }: Props = $props();

  let scale = $state(1);
  let translateX = $state(0);
  let translateY = $state(0);
  let dragging = $state(false);
  let lastX = 0;
  let lastY = 0;

  let imgSrc = $derived(rawUrl(path));

  function handleWheel(e: WheelEvent) {
    e.preventDefault();
    const delta = e.deltaY > 0 ? -0.1 : 0.1;
    scale = Math.max(0.1, Math.min(10, scale + delta));
  }

  function handleMouseDown(e: MouseEvent) {
    dragging = true;
    lastX = e.clientX;
    lastY = e.clientY;
  }

  function handleMouseMove(e: MouseEvent) {
    if (!dragging) return;
    translateX += e.clientX - lastX;
    translateY += e.clientY - lastY;
    lastX = e.clientX;
    lastY = e.clientY;
  }

  function handleMouseUp() {
    dragging = false;
  }

  function resetView() {
    scale = 1;
    translateX = 0;
    translateY = 0;
  }
</script>

<svelte:window onmouseup={handleMouseUp} onmousemove={handleMouseMove} />

<div
  class="h-full overflow-hidden flex items-center justify-center relative"
  style="background: var(--bg); cursor: {dragging ? 'grabbing' : 'grab'};"
  role="application"
  onwheel={handleWheel}
  onmousedown={handleMouseDown}
>
  <img
    src={imgSrc}
    alt={path.split('/').pop()}
    style="transform: translate({translateX}px, {translateY}px) scale({scale}); transform-origin: center; max-width: none; max-height: none; user-select: none; pointer-events: none;"
    draggable="false"
  />

  <!-- Controls -->
  <div class="viewer-controls">
    <button class="icon-button viewer-control" onclick={() => { scale = Math.max(0.1, scale - 0.25); }} title="Zoom out" aria-label="Zoom out">
      <ZoomOut size={16} />
    </button>
    <span class="viewer-zoom">{Math.round(scale * 100)}%</span>
    <button class="icon-button viewer-control" onclick={() => { scale = Math.min(10, scale + 0.25); }} title="Zoom in" aria-label="Zoom in">
      <ZoomIn size={16} />
    </button>
    <button class="icon-button viewer-control" onclick={resetView} title="Reset view" aria-label="Reset view">
      <RotateCcw size={16} />
    </button>
  </div>
</div>
