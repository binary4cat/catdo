<script lang="ts">
  import { BookOpen } from 'lucide-svelte';
  import { Toaster } from 'svelte-sonner';
  import { ModeWatcher } from 'mode-watcher';
  import Sidebar from './lib/components/Sidebar.svelte';
  import TabBar from './lib/components/TabBar.svelte';
  import EditorContainer from './lib/components/EditorContainer.svelte';
  import ThemeToggle from './lib/components/ThemeToggle.svelte';
  import {
    getActiveTab,
    getTabs,
    zen,
    getSidebarCollapsed,
    toggleSidebar,
  } from './lib/stores/workspace.svelte';

  let activeTab = $derived(getActiveTab());
  let collapsed = $derived(getSidebarCollapsed());
  let activeModified = $derived(Boolean(getTabs().find((tab) => tab.path === activeTab)?.modified));
</script>

<ModeWatcher />
<Toaster richColors position="bottom-right" />

{#if zen}
  <div class="zen-shell">
    <div class="zen-toolbar">
      <span
        class="save-dot"
        class:saved={!activeModified}
        title={activeModified ? 'Unsaved changes' : 'Saved'}
        aria-label={activeModified ? 'Unsaved changes' : 'Saved'}
      ></span>
      <ThemeToggle />
    </div>
    <main class="zen-editor">
      <div class="editor-measure">
        {#if activeTab}
          {#key activeTab}
            <EditorContainer path={activeTab} />
          {/key}
        {:else}
          <div class="empty-state">
            <span class="empty-state-icon"><BookOpen size={24} strokeWidth={1.8} /></span>
            <strong>Open a note to begin</strong>
          </div>
        {/if}
      </div>
    </main>
  </div>
{:else}
  <div class="app-shell">
    {#if !collapsed}<Sidebar />{/if}

    <main class="main-shell">
      <TabBar onsidebarToggle={toggleSidebar} sidebarCollapsed={collapsed} />
      <section class="editor-stage">
        {#if activeTab}
          {#key activeTab}
            <EditorContainer path={activeTab} />
          {/key}
        {:else}
          <div class="empty-state">
            <span class="empty-state-icon"><BookOpen size={26} strokeWidth={1.8} /></span>
            <strong>Open a note to begin</strong>
            <span>Choose a file from your vault</span>
          </div>
        {/if}
      </section>
      <footer class="status-bar">
        <span class="status-path">{activeTab ?? 'No note selected'}</span>
        <span class:status-unsaved={activeModified}>{activeModified ? 'Unsaved changes' : activeTab ? 'Saved' : ''}</span>
      </footer>
    </main>
  </div>
{/if}
