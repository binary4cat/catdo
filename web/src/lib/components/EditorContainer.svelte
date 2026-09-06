<script lang="ts">
  import MilkdownEditor from './MilkdownEditor.svelte';
  import PdfViewer from './PdfViewer.svelte';
  import ImageViewer from './ImageViewer.svelte';

  interface Props {
    path: string;
  }

  let { path }: Props = $props();

  const imageExts = ['.png', '.jpg', '.jpeg', '.gif', '.svg', '.webp', '.bmp', '.ico'];
  let fileType = $derived.by(() => {
    const lower = path.toLowerCase();
    if (lower.endsWith('.pdf')) return 'pdf' as const;
    if (imageExts.some((ext) => lower.endsWith(ext))) return 'image' as const;
    return 'markdown' as const;
  });
</script>

<div class="h-full">
  {#if fileType === 'pdf'}
    <PdfViewer {path} />
  {:else if fileType === 'image'}
    <ImageViewer {path} />
  {:else}
    <MilkdownEditor {path} />
  {/if}
</div>
