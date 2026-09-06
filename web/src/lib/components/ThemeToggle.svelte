<script lang="ts">
  import { Moon, Sun } from 'lucide-svelte';
  import { toggleMode, mode } from 'mode-watcher';

  let currentMode = $derived(mode);
</script>

<button
  class="icon-button theme-toggle"
  onclick={toggleMode}
  title={currentMode === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
  aria-label={currentMode === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
>
  {#if currentMode === 'dark'}
    <Sun size={16} strokeWidth={1.9} />
  {:else}
    <Moon size={16} strokeWidth={1.9} />
  {/if}
</button>
