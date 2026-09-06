<script lang="ts">
  import { onMount } from 'svelte';
  import { rawUrl } from '../api';

  interface Props {
    path: string;
  }

  let { path }: Props = $props();

  let canvasContainer: HTMLDivElement;
  let loading = $state(true);
  let error = $state('');
  let numPages = $state(0);
  let currentPage = $state(1);

  async function renderPdf() {
    loading = true;
    error = '';

    try {
      const pdfjsLib = await import('pdfjs-dist');
      // Use bundled worker
      pdfjsLib.GlobalWorkerOptions.workerSrc = new URL(
        'pdfjs-dist/build/pdf.worker.min.mjs',
        import.meta.url,
      ).toString();

      const url = rawUrl(path);
      const pdf = await pdfjsLib.getDocument(url).promise;
      numPages = pdf.numPages;

      // Clear previous canvases
      canvasContainer.innerHTML = '';

      for (let i = 1; i <= pdf.numPages; i++) {
        const page = await pdf.getPage(i);
        const scale = 1.5;
        const viewport = page.getViewport({ scale });

        const canvas = document.createElement('canvas');
        canvas.classList.add('pdf-canvas');
        canvas.width = viewport.width;
        canvas.height = viewport.height;
        canvas.style.display = 'block';
        canvas.style.margin = '0 auto 16px';
        canvas.style.maxWidth = '100%';

        canvasContainer.appendChild(canvas);

        const ctx = canvas.getContext('2d')!;
        await page.render({ canvasContext: ctx, viewport }).promise;
      }

      loading = false;
    } catch (e) {
      error = 'Failed to load PDF';
      loading = false;
      console.error(e);
    }
  }

  $effect(() => {
    if (path && canvasContainer) {
      renderPdf();
    }
  });
</script>

<div class="h-full overflow-auto p-4">
  {#if loading}
    <div class="flex items-center justify-center h-32 text-[var(--muted)]">
      Loading PDF...
    </div>
  {/if}
  {#if error}
    <div class="flex items-center justify-center h-32 text-red-500">
      {error}
    </div>
  {/if}
  <div bind:this={canvasContainer}></div>
</div>
