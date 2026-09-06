<script lang="ts">
  import { ExternalLink, PanelLeft, X } from 'lucide-svelte';
  import { getTabs, getActiveTab, openFile, closeTab } from '../stores/workspace.svelte';

  interface Props {
    onsidebarToggle: () => void;
    sidebarCollapsed: boolean;
  }

  let { onsidebarToggle, sidebarCollapsed }: Props = $props();
  let tabs = $derived(getTabs());
  let activeTab = $derived(getActiveTab());
  let contextMenu = $state<{ x: number; y: number; path: string } | null>(null);

  function handleContextMenu(event: MouseEvent, path: string) {
    event.preventDefault();
    contextMenu = { x: event.clientX, y: event.clientY, path };
  }

  function openInZen(path: string) {
    window.open(`/?file=${encodeURIComponent(path)}&zen=true`, '_blank');
    contextMenu = null;
  }

  function handleWindowClick() {
    contextMenu = null;
  }
</script>

<svelte:window onclick={handleWindowClick} />

<div class="tabbar-shell">
  <button
    class="icon-button tabbar-toggle"
    onclick={onsidebarToggle}
    title={sidebarCollapsed ? 'Show sidebar' : 'Hide sidebar'}
    aria-label={sidebarCollapsed ? 'Show sidebar' : 'Hide sidebar'}
  >
    <PanelLeft size={16} strokeWidth={2} />
  </button>

  <div class="tab-strip">
    {#each tabs as tab}
      <button
        class="tab-item"
        class:active={activeTab === tab.path}
        onclick={() => openFile(tab.path)}
        oncontextmenu={(event) => handleContextMenu(event, tab.path)}
        title={tab.path}
      >
        <span class="tab-label">{tab.name}</span>
        {#if tab.modified}<span class="tab-dirty" title="Unsaved changes"></span>{/if}
        <span
          class="tab-close"
          role="button"
          tabindex="0"
          onclick={(event) => { event.stopPropagation(); closeTab(tab.path); }}
          onkeydown={(event) => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); event.stopPropagation(); closeTab(tab.path); } }}
          aria-label={`Close ${tab.name}`}
        >
          <X size={14} strokeWidth={2} />
        </span>
      </button>
    {/each}
    {#if tabs.length === 0}<span class="tab-empty">Select a note to start writing</span>{/if}
  </div>
</div>

{#if contextMenu}
  <div
    class="context-menu"
    style={`left: ${contextMenu.x}px; top: ${contextMenu.y}px`}
    role="menu"
  >
    <button class="context-menu-item" onclick={() => openInZen(contextMenu!.path)}>
      <ExternalLink size={15} />
      <span>Open in Zen mode</span>
    </button>
    <button class="context-menu-item danger" onclick={() => { closeTab(contextMenu!.path); contextMenu = null; }}>
      <X size={15} />
      <span>Close tab</span>
    </button>
  </div>
{/if}
