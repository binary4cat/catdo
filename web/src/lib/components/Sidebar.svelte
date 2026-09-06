<script lang="ts">
  import {
    BookOpen,
    ChevronRight,
    File as FileIcon,
    FileImage,
    FileText,
    Folder,
    LoaderCircle,
    Plus,
    Search,
    X,
  } from 'lucide-svelte';
  import {
    getDirectoryExpanded,
    getDirectoryLoaded,
    getDirectoryLoading,
    getTree,
    getTreeLoading,
    getActiveTab,
    loadDirectory,
    openFile,
    refreshTree,
    setDirectoryExpanded,
  } from '../stores/workspace.svelte';
  import { searchFiles, saveFile } from '../api';
  import { toast } from 'svelte-sonner';
  import ThemeToggle from './ThemeToggle.svelte';
  import type { FileNode } from '../types';

  let tree = $derived(getTree());
  let loading = $derived(getTreeLoading());
  let activeTab = $derived(getActiveTab());
  let searchQuery = $state('');
  let searchResults = $state<FileNode[]>([]);
  let searching = $state(false);
  let newFileName = $state('');
  let showNewFile = $state(false);
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let searchRequest = 0;

  const imageExtensions = ['.png', '.jpg', '.jpeg', '.gif', '.svg', '.webp', '.bmp', '.ico'];

  function handleDirectoryToggle(node: FileNode, event: Event) {
    const details = event.currentTarget as HTMLDetailsElement;
    setDirectoryExpanded(node.path, details.open);
    if (details.open) void loadDirectory(node.path);
  }

  function updateSearch(value: string) {
    searchQuery = value;
    if (searchTimer) clearTimeout(searchTimer);
    if (!value.trim()) {
      searchResults = [];
      searching = false;
      return;
    }

    const request = ++searchRequest;
    searching = true;
    searchTimer = setTimeout(async () => {
      try {
        const results = await searchFiles(value);
        if (request === searchRequest) searchResults = results;
      } catch {
        if (request === searchRequest) toast.error('Search failed');
      } finally {
        if (request === searchRequest) searching = false;
      }
    }, 180);
  }

  async function createNewFile() {
    const requested = newFileName.trim();
    if (!requested) return;
    const path = requested.endsWith('.md') ? requested : `${requested}.md`;
    try {
      await saveFile(path, '');
      const parent = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
      await refreshTree(parent);
      openFile(path);
      newFileName = '';
      showNewFile = false;
      toast.success(`Created ${path}`);
    } catch {
      toast.error('Failed to create note');
    }
  }
</script>

<aside class="sidebar-shell">
  <header class="sidebar-header">
    <div class="brand-lockup">
      <span class="brand-mark"><BookOpen size={16} strokeWidth={2.2} /></span>
      <span class="brand-name">catdo</span>
    </div>
    <ThemeToggle />
  </header>

  <div class="sidebar-content">
    <label class="search-field">
      <Search size={15} strokeWidth={2} />
      <input
        value={searchQuery}
        oninput={(event) => updateSearch((event.currentTarget as HTMLInputElement).value)}
        placeholder="Search your vault"
        aria-label="Search your vault"
      />
      {#if searching}
        <LoaderCircle class="spin" size={15} strokeWidth={2} />
      {:else if searchQuery}
        <button class="icon-button search-clear" onclick={() => updateSearch('')} aria-label="Clear search">
          <X size={14} />
        </button>
      {/if}
    </label>

    {#if showNewFile}
      <form class="new-note-form" onsubmit={(event) => { event.preventDefault(); void createNewFile(); }}>
        <input bind:value={newFileName} placeholder="path/to/note.md" aria-label="New note path" autofocus />
        <button class="primary-button compact" type="submit">Create</button>
        <button class="icon-button subtle" type="button" onclick={() => { showNewFile = false; }} aria-label="Cancel">
          <X size={15} />
        </button>
      </form>
    {:else}
      <button class="new-note-button" onclick={() => { showNewFile = true; }}>
        <Plus size={15} strokeWidth={2.2} />
        <span>New note</span>
      </button>
    {/if}

    <div class="tree-heading">
      <span>Vault</span>
      {#if !searchQuery}<span class="tree-hint">lazy loaded</span>{/if}
    </div>

    <div class="tree-scroll">
      {#if searchQuery}
        {#if searching}
          <div class="tree-empty"><LoaderCircle class="spin" size={16} /> Searching…</div>
        {:else if searchResults.length === 0}
          <div class="tree-empty">No matching files</div>
        {:else}
          {#each searchResults as result}
            {@render fileNode(result, 0)}
          {/each}
        {/if}
      {:else if loading}
        <div class="tree-empty"><LoaderCircle class="spin" size={16} /> Loading vault…</div>
      {:else if tree.length === 0}
        <div class="tree-empty">Your vault is empty</div>
      {:else}
        {#each tree as node}
          {@render treeNode(node, 0)}
        {/each}
      {/if}
    </div>
  </div>
</aside>

{#snippet treeNode(node: FileNode, depth: number)}
  {#if node.is_dir}
    <details
      class="tree-folder"
      open={getDirectoryExpanded(node.path)}
      ontoggle={(event) => handleDirectoryToggle(node, event)}
    >
      <summary class="tree-row" style={`padding-left: ${depth * 14 + 8}px`}>
        <ChevronRight class="tree-chevron" size={14} strokeWidth={2} />
        <Folder class="tree-folder-icon" size={15} strokeWidth={1.9} />
        <span class="tree-label">{node.name}</span>
        {#if getDirectoryLoading(node.path)}<LoaderCircle class="spin tree-loader" size={13} />{/if}
      </summary>
      {#if getDirectoryLoaded(node.path)}
        {#each node.children ?? [] as child}
          {@render treeNode(child, depth + 1)}
        {/each}
      {/if}
    </details>
  {:else}
    {@render fileNode(node, depth)}
  {/if}
{/snippet}

{#snippet fileNode(node: FileNode, depth: number)}
  <button
    class="tree-row tree-file"
    class:active={activeTab === node.path}
    style={`padding-left: ${depth * 14 + 30}px`}
    onclick={() => openFile(node.path)}
    title={node.path}
  >
    {#if node.name.toLowerCase().endsWith('.pdf')}
      <FileText class="tree-file-icon" size={15} strokeWidth={1.9} />
    {:else if imageExtensions.some((extension) => node.name.toLowerCase().endsWith(extension))}
      <FileImage class="tree-file-icon" size={15} strokeWidth={1.9} />
    {:else}
      <FileIcon class="tree-file-icon" size={15} strokeWidth={1.9} />
    {/if}
    <span class="tree-label">{node.name}</span>
  </button>
{/snippet}
