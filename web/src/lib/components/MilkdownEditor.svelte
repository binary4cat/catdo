<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { Crepe } from '@milkdown/crepe';
  import { editorViewCtx, serializerCtx } from '@milkdown/kit/core';
  import { imageBlockSchema } from '@milkdown/kit/component/image-block';
  import { listener, listenerCtx } from '@milkdown/kit/plugin/listener';
  import { fetchFile, rawUrl, saveFile, patchFile, uploadAsset } from '../api';
  import {
    cacheFileContent,
    getAllMarkdownPaths,
    getCachedFileContent,
    markTabModified,
    openFile,
    ensureMarkdownPaths,
  } from '../stores/workspace.svelte';
  import { buildWikiLinkPlugins } from '../milkdown-plugins';
  import { normalizeMarkdown } from '../wikilink';
  import {
    saveDraft,
    loadDraft,
    deleteDraft,
    requestPersistence,
  } from '../draft-journal';
  import { computeLineDiff, generateRequestId } from '../line-diff';
  import '@milkdown/crepe/theme/common/style.css';
  import '@milkdown/crepe/theme/frame.css';
  import { toast } from 'svelte-sonner';

  interface Props {
    path: string;
  }

  let { path }: Props = $props();

  let editorHost: HTMLDivElement;
  let crepe: Crepe | null = null;
  let currentContent = '';
  let lastSavedContent = '';
  let loadedVersion: string | null = null;
  let saveTimer: number | null = null;
  let saveInFlight = false;
  let pendingSave: {
    path: string;
    content: string;
    baseVersion: string;
    token: number;
    revision: number;
  } | null = null;
  let saveBlocked = false;
  let editRevision = 0;
  let teardownSave = false;
  let loadGeneration = 0;
  let disposed = false;
  let mounted = false;
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let cachedContent = $state<string | null>(null);
  let editorReady = false;
  let draftConflict = $state(false);
  let staleDraftContent = $state<string | null>(null);
  let staleDraftBaseVersion = $state<string | null>(null);
  let forcedInitialContent: string | null = null;
  function getAssetDir(filePath: string): string {
    const parts = filePath.split('/');
    parts.pop(); // remove filename
    const dir = parts.length > 0 ? parts.join('/') + '/_assets' : '_assets';
    return dir;
  }

  function generateImageName(): string {
    const now = new Date();
    const pad = (n: number) => n.toString().padStart(2, '0');
    return `Pasted_image_${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}_${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}.png`;
  }

  async function uploadImage(file: File): Promise<string> {
    const targetDir = getAssetDir(path);
    const timestampName = generateImageName();
    let fileName = timestampName;
    try {
      let relativePath: string | null = null;
      for (let attempt = 0; attempt < 3 && relativePath === null; attempt += 1) {
        try {
          relativePath = await uploadAsset(targetDir, file, fileName);
        } catch (error) {
          if ((error as { status?: number }).status !== 409 || attempt === 2) throw error;
          fileName = `${timestampName.replace(/\.png$/i, '')}_${crypto.randomUUID().slice(0, 8)}.png`;
        }
      }

      const noteDir = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
      const prefix = noteDir ? `${noteDir}/` : '';
      const assetRef = relativePath!.startsWith(prefix)
        ? relativePath!.slice(prefix.length)
        : relativePath!;
      toast.success('Image uploaded');
      return assetRef;
    } catch (error) {
      toast.error('Failed to upload image');
      throw error;
    }
  }

  async function insertUploadedImage(file: File) {
    const assetRef = await uploadImage(file);
    if (!crepe) return;

    crepe.editor.action((ctx) => {
      const view = ctx.get(editorViewCtx);
      const image = imageBlockSchema.type(ctx).create({
        src: assetRef,
        caption: '',
        ratio: 1,
      });
      const insertPos = view.state.selection.$from.after(view.state.selection.$from.depth);
      view.dispatch(view.state.tr.insert(insertPos, image));
    });
  }

  function handleDrop(event: DragEvent) {
    const file = Array.from(event.dataTransfer?.files ?? []).find((candidate) =>
      candidate.type.startsWith('image/'),
    );
    if (!file) return;
    event.preventDefault();
    event.stopPropagation();
    void insertUploadedImage(file);
  }

  function isCurrent(token: number) {
    return mounted && !disposed && token === loadGeneration;
  }

  function queueSaveFlush(delay = 1000) {
    if (saveTimer !== null) window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => {
      saveTimer = null;
      void flushPendingSave();
    }, delay);
  }

  function scheduleSave(content: string) {
    if (!editorReady || loadedVersion === null || disposed) return;
    const normalized = normalizeMarkdown(content);
    if (draftConflict) staleDraftContent = normalized;
    const baseVersion = draftConflict
      ? staleDraftBaseVersion ?? loadedVersion
      : loadedVersion;
    const revision = ++editRevision;
    pendingSave = {
      path,
      content: normalized,
      baseVersion,
      token: loadGeneration,
      revision,
    };
    markTabModified(path, true);

    void saveDraft({
      path,
      content: normalized,
      baseVersion,
      localRevision: revision,
      updatedAt: Date.now(),
      saveState: 'dirty',
    }).catch(() => undefined);

    if (!saveBlocked) queueSaveFlush();
  }

  async function flushPendingSave(allowStale = false) {
    if (saveInFlight) {
      if (pendingSave && allowStale) teardownSave = true;
      else if (pendingSave && !saveBlocked) queueSaveFlush(250);
      return;
    }

    const pending = pendingSave;
    pendingSave = null;
    if (!pending || saveBlocked || (!allowStale && !isCurrent(pending.token))) return;

    const savePath = pending.path;
    const expectedVersion = pending.baseVersion;
    const baseContent = lastSavedContent;
    await saveDraft({
      path: savePath,
      content: pending.content,
      baseVersion: expectedVersion,
      localRevision: pending.revision,
      updatedAt: Date.now(),
      saveState: 'saving',
    }).catch(() => undefined);
    saveInFlight = true;
    const requestId = generateRequestId();

    try {
      let saved: { version: string };
      const ops = await computeLineDiff(baseContent, pending.content);
      if (ops.length > 0) {
        try {
          saved = await patchFile(savePath, expectedVersion, ops, requestId);
        } catch (patchError) {
          const patchStatus = (patchError as { status?: number }).status;
          if (patchStatus === 422) {
            saved = await saveFile(savePath, pending.content, expectedVersion, requestId);
          } else {
            throw patchError;
          }
        }
      } else {
        saved = await saveFile(savePath, pending.content, expectedVersion, requestId);
      }

      cacheFileContent({ path: savePath, content: pending.content, modified: 0, version: saved.version });
      await deleteDraft(savePath, pending.revision).catch(() => undefined);

      if (!isCurrent(pending.token)) return;
      loadedVersion = saved.version;
      lastSavedContent = pending.content;
      if (pending.revision === editRevision) {
        markTabModified(savePath, false);
      }
    } catch (error) {
      await saveDraft({
        path: savePath,
        content: pending.content,
        baseVersion: expectedVersion,
        localRevision: pending.revision,
        updatedAt: Date.now(),
        saveState: 'error',
      }).catch(() => undefined);

      if (!allowStale && !isCurrent(pending.token)) return;
      if ((error as { status?: number }).status === 409) {
        saveBlocked = true;
        draftConflict = true;
        staleDraftContent = pending.content;
        staleDraftBaseVersion = expectedVersion;
        if (isCurrent(pending.token)) toast.error('File changed externally; local draft was preserved');
      } else if (isCurrent(pending.token)) {
        toast.error('Failed to save; local draft was preserved');
      }
    } finally {
      saveInFlight = false;
      if (pendingSave && !saveBlocked) {
        if (teardownSave) {
          teardownSave = false;
          if (saveTimer !== null) window.clearTimeout(saveTimer);
          saveTimer = null;
          void flushPendingSave(true);
        } else if (saveTimer === null && isCurrent(pending.token)) {
          queueSaveFlush();
        }
      }
    }
  }

  function recoverLocalDraft() {
    if (!staleDraftContent) return;
    forcedInitialContent = staleDraftContent;
    staleDraftContent = null;
    staleDraftBaseVersion = null;
    draftConflict = false;
    saveBlocked = true;
    stopEditor();
    void initEditor();
  }

  async function discardLocalDraft() {
    saveBlocked = true;
    try {
      await deleteDraft(path);
      staleDraftContent = null;
      draftConflict = false;
      stopEditor();
      void initEditor();
    } catch {
      saveBlocked = true;
      toast.error('Could not discard the local draft; it was kept safely');
    }
  }

  async function initEditor() {
    const token = ++loadGeneration;
    loading = true;
    loadError = null;
    cachedContent = null;
    editorReady = false;
    loadedVersion = null;
    saveBlocked = false;
    pendingSave = null;
    editRevision = 0;
    draftConflict = false;
    staleDraftContent = null;
    staleDraftBaseVersion = null;
    try {
      const fileData = await fetchFile(path);
      if (!isCurrent(token)) return;

      const forcedContent = forcedInitialContent;
      forcedInitialContent = null;
      const draft = await loadDraft(path);
      let initialContent = forcedContent ?? fileData.content;
      if (
        forcedContent === null &&
        draft &&
        (draft.saveState === 'dirty' || draft.saveState === 'saving' || draft.saveState === 'error') &&
        draft.content !== fileData.content
      ) {
        if (draft.baseVersion === fileData.version) {
          initialContent = draft.content;
          toast.info('Recovered unsaved changes from previous session');
        } else {
          initialContent = draft.content;
          draftConflict = true;
          saveBlocked = true;
          staleDraftContent = draft.content;
          staleDraftBaseVersion = draft.baseVersion;
          toast.warning('File changed externally; local draft was preserved for review');
        }
      } else if (forcedContent === null && draft) {
        await deleteDraft(path).catch(() => undefined);
      }
      const editorContent = initialContent.replace(/\r\n?/g, '\n');

      currentContent = initialContent;
      lastSavedContent = fileData.content;
      loadedVersion = fileData.version;
      cacheFileContent(fileData);
      editorHost.replaceChildren();

      crepe = new Crepe({
        defaultValue: editorContent,
        featureConfigs: {
          [Crepe.Feature.ImageBlock]: {
            onUpload: uploadImage,
            proxyDomURL: (url: string) => {
              if (!url.startsWith('_assets/')) return url;
              const noteDir = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
              return rawUrl(noteDir ? `${noteDir}/${url}` : url);
            },
          },
        },
      });

      function navigateWikiLink(target: string) {
        let targetPath = target;
        if (!targetPath.endsWith('.md')) targetPath += '.md';

        const knownPaths = getAllMarkdownPaths();
        const match = knownPaths.find(
          (candidate) =>
            candidate === targetPath ||
            candidate.endsWith('/' + targetPath) ||
            candidate.split('/').pop() === target + '.md',
        );

        if (match) {
          openFile(match);
        } else {
          void saveFile(targetPath, '')
            .then(() => openFile(targetPath))
            .catch((error) => {
              toast.error((error as { status?: number }).status === 409
                ? 'File already exists; refresh the file list'
                : 'Failed to create note');
            });
        }
      }

      await ensureMarkdownPaths();
      if (!isCurrent(token)) return;
      const { decoPlugin, inputPlugin } = buildWikiLinkPlugins(
        navigateWikiLink,
        getAllMarkdownPaths,
      );

      crepe.editor.use(decoPlugin);
      crepe.editor.use(inputPlugin);
      crepe.editor.use(listener);

      crepe.editor.config((ctx) => {
        ctx.get(listenerCtx).markdownUpdated((_ctx, markdown, prevMarkdown) => {
          if (markdown !== prevMarkdown) {
            currentContent = markdown;
            scheduleSave(markdown);
          }
        });
      });

      await crepe.create();
      if (!isCurrent(token)) {
        crepe.destroy();
        crepe = null;
        return;
      }
      editorReady = true;
      loading = false;
      // A stale conflict draft stays local-only until the user chooses recovery.
      if (editorContent !== fileData.content && !draftConflict) {
        scheduleSave(editorContent);
      }
    } catch (error) {
      if (!isCurrent(token)) return;
      console.error('Failed to init editor:', error);
      if (crepe) {
        try {
          crepe.destroy();
        } catch {
          // Ignore partial editor cleanup failures.
        }
        crepe = null;
      }
      editorHost.replaceChildren();
      cachedContent = getCachedFileContent(path)?.content ?? null;
      loadError = 'Unable to open this file safely. No changes were written.';
      loading = false;
      toast.error('Failed to load file; nothing was saved');
    }
  }

  // Handle path changes: destroy and recreate.
  function captureEditorContent() {
    if (!editorReady || loadedVersion === null || !crepe || saveBlocked) return;
    let serialized: string | null = null;
    try {
      crepe.editor.action((ctx) => {
        const view = ctx.get(editorViewCtx);
        const serializer = ctx.get(serializerCtx);
        serialized = serializer(view.state.doc);
      });
    } catch {
      return;
    }
    if (serialized === null) return;
    const normalized = normalizeMarkdown(serialized);
    if (normalized === normalizeMarkdown(lastSavedContent)) return;
    currentContent = serialized;
    pendingSave = { content: normalized, token: loadGeneration, revision: ++editRevision };
    markTabModified(path, true);

    // Persist draft immediately
    void saveDraft({
      path,
      content: normalized,
      baseVersion: loadedVersion!,
      localRevision: editRevision,
      updatedAt: Date.now(),
      saveState: 'dirty',
    });
  }

  let currentPath = '';

  function stopEditor() {
    captureEditorContent();
    if (saveTimer !== null) {
      window.clearTimeout(saveTimer);
      saveTimer = null;
    }
    if (pendingSave && editorReady && loadedVersion !== null && !saveBlocked) {
      if (saveInFlight) teardownSave = true;
      else void flushPendingSave(true);
    }
    editorReady = false;
    ++loadGeneration;
    if (crepe) {
      crepe.destroy();
      crepe = null;
    }
  }

  $effect(() => {
    if (path !== currentPath) {
      currentPath = path;
      stopEditor();
      if (editorHost) void initEditor();
    }
  });

  // --- Visibility and beforeunload handlers ---

  function handleVisibilityChange() {
    if (document.hidden && pendingSave && !saveBlocked && !saveInFlight) {
      // Page going hidden — flush immediately
      if (saveTimer !== null) {
        window.clearTimeout(saveTimer);
        saveTimer = null;
      }
      void flushPendingSave();
    }
  }

  function handleBeforeUnload(event: BeforeUnloadEvent) {
    // captureEditorContent synchronously updates the localStorage shadow;
    // asynchronous network/IndexedDB work is not reliable during unload.
    captureEditorContent();
    if (pendingSave && !saveBlocked) {
      event.preventDefault();
      event.returnValue = '';
    }
  }

  onMount(() => {
    mounted = true;
    void requestPersistence();
    document.addEventListener('drop', handleDrop, true);
    document.addEventListener('visibilitychange', handleVisibilityChange);
    window.addEventListener('beforeunload', handleBeforeUnload);
    if (!crepe && editorHost) {
      currentPath = path;
      void initEditor();
    }
  });

  onDestroy(() => {
    captureEditorContent();
    if (pendingSave && editorReady && loadedVersion !== null && !saveBlocked) {
      if (saveInFlight) teardownSave = true;
      else void flushPendingSave(true);
    }
    disposed = true;
    mounted = false;
    document.removeEventListener('drop', handleDrop, true);
    document.removeEventListener('visibilitychange', handleVisibilityChange);
    window.removeEventListener('beforeunload', handleBeforeUnload);
    if (saveTimer !== null) window.clearTimeout(saveTimer);
    saveTimer = null;
    editorReady = false;
    ++loadGeneration;
    if (crepe) {
      crepe.destroy();
      crepe = null;
    }
  });
</script>

<div class="h-full overflow-auto milkdown-editor">
  {#if draftConflict}
    <div class="mx-auto mt-3 flex max-w-4xl flex-wrap items-center gap-2 rounded border border-amber-400/50 bg-amber-50 px-3 py-2 text-sm text-amber-950 dark:bg-amber-950/40 dark:text-amber-100">
      <span class="mr-auto">检测到文件已被外部修改；本地草稿已保留，保存暂时锁定。</span>
      <button class="rounded border px-2 py-1" onclick={recoverLocalDraft}>恢复本地草稿并覆盖当前版本</button>
      <button class="rounded border px-2 py-1" onclick={() => void discardLocalDraft()}>放弃本地草稿，使用磁盘版本</button>
    </div>
  {/if}
  <div class="editor-host h-full" bind:this={editorHost}></div>
  {#if loading}
    <div class="editor-load-status">
      Loading…
    </div>
  {:else if loadError}
    <div class="editor-load-error">
      <strong>File not opened</strong>
      <span>{loadError}</span>
      {#if cachedContent}
        <pre>{cachedContent}</pre>
      {/if}
    </div>
  {/if}
</div>
