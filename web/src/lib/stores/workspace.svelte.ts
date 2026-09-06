import type { FileContent, FileNode, TabItem, WsMessage } from '../types';
import { fetchMarkdownPaths, fetchTree, connectWs } from '../api';

let tree = $state<FileNode[]>([]);
let treeLoading = $state(true);
let directoryLoading = $state<Record<string, boolean>>({});
let directoryLoaded = $state<Record<string, boolean>>({});
let expandedDirectories = $state<Record<string, boolean>>({});
const directoryRefreshTimers: Record<string, number | undefined> = {};
let allMarkdownPaths = $state<string[]>([]);
let markdownPathsLoaded = false;
let markdownPathsLoading = false;
const fileContentCache: Record<string, { content: string; version: string }> = {};
let tabs = $state<TabItem[]>([]);
let activeTab = $state<string | null>(null);
let _sidebarCollapsed = $state(false);

const params = new URLSearchParams(window.location.search);
export const zen: boolean = params.get('zen') === 'true';
const initialFile = params.get('file');

function mergeLoadedChildren(previous: FileNode[] | null | undefined, fresh: FileNode[]): FileNode[] {
  const previousByPath: Record<string, FileNode> = {};
  for (const node of previous ?? []) previousByPath[node.path] = node;
  return fresh.map((node) => {
    const oldNode = previousByPath[node.path];
    if (node.is_dir && oldNode && Array.isArray(oldNode.children)) {
      return { ...node, children: oldNode.children };
    }
    return node;
  });
}

function findNode(nodes: FileNode[], path: string): FileNode | undefined {
  for (const node of nodes) {
    if (node.path === path) return node;
    if (node.children) {
      const match = findNode(node.children, path);
      if (match) return match;
    }
  }
  return undefined;
}

function replaceChildren(nodes: FileNode[], path: string, children: FileNode[]): FileNode[] {
  return nodes.map((node) => {
    if (node.path === path) {
      return { ...node, children: mergeLoadedChildren(node.children, children) };
    }
    if (node.children) {
      return { ...node, children: replaceChildren(node.children, path, children) };
    }
    return node;
  });
}

function directoryHasLoadedChildren(path: string): boolean {
  return path ? Array.isArray(findNode(tree, path)?.children) : directoryLoaded[''] ?? false;
}
export async function loadDirectory(path = '', force = false) {
  if (directoryLoading[path] || (!force && directoryHasLoadedChildren(path))) return;
  directoryLoading[path] = true;
  if (!path) treeLoading = true;

  try {
    const children = await fetchTree(path);
    tree = path ? replaceChildren(tree, path, children) : mergeLoadedChildren(tree, children);
    directoryLoaded[path] = true;
  } catch (error) {
    console.error(`Failed to load directory: ${path || '/'}`, error);
  } finally {
    directoryLoading[path] = false;
    if (!path) treeLoading = false;
  }
}

function scheduleTreeRefresh(path: string) {
  if (directoryRefreshTimers[path] !== undefined) {
    window.clearTimeout(directoryRefreshTimers[path]);
  }
  directoryRefreshTimers[path] = window.setTimeout(() => {
    directoryRefreshTimers[path] = undefined;
    void refreshTree(path);
  }, 150);
}

export async function refreshTree(path = '') {
  if (directoryLoading[path]) {
    scheduleTreeRefresh(path);
    return;
  }
  await loadDirectory(path, true);
  allMarkdownPaths = [];
  markdownPathsLoaded = false;
}

export async function ensureMarkdownPaths() {
  if (markdownPathsLoaded || markdownPathsLoading) return;
  markdownPathsLoading = true;
  try {
    allMarkdownPaths = await fetchMarkdownPaths();
    markdownPathsLoaded = true;
  } catch (error) {
    console.error('Failed to load markdown paths', error);
  } finally {
    markdownPathsLoading = false;
  }
}

export function openFile(path: string) {
  const name = path.split('/').pop() || path;
  if (!tabs.find((tab) => tab.path === path)) {
    tabs.push({ path, name, modified: false });
  }
  activeTab = path;
  const url = new URL(window.location.href);
  url.searchParams.set('file', path);
  window.history.replaceState({}, '', url.toString());
}

export function closeTab(path: string) {
  tabs = tabs.filter((tab) => tab.path !== path);
  if (activeTab === path) {
    activeTab = tabs.length > 0 ? tabs[tabs.length - 1].path : null;
  }
}

export function markTabModified(path: string, modified: boolean) {
  const tab = tabs.find((tab) => tab.path === path);
  if (tab) tab.modified = modified;
}

export function cacheFileContent(file: FileContent) {
  fileContentCache[file.path] = { content: file.content, version: file.version };
}

export function getCachedFileContent(path: string) {
  return fileContentCache[path];
}

export function toggleSidebar() {
  _sidebarCollapsed = !_sidebarCollapsed;
}

export function getTree() { return tree; }
export function getTreeLoading() { return treeLoading; }
export function getDirectoryLoading(path: string) { return directoryLoading[path] ?? false; }
export function getDirectoryExpanded(path: string) { return expandedDirectories[path] ?? false; }
export function setDirectoryExpanded(path: string, expanded: boolean) {
  expandedDirectories[path] = expanded;
}
export function getDirectoryLoaded(path: string) { return directoryHasLoadedChildren(path); }
export function getTabs() { return tabs; }
export function getActiveTab() { return activeTab; }
export function getSidebarCollapsed() { return _sidebarCollapsed; }
export function getAllMarkdownPaths() { return allMarkdownPaths; }

function parentDirectory(path: string) {
  const separator = path.lastIndexOf('/');
  return separator === -1 ? '' : path.slice(0, separator);
}

function initWs() {
  connectWs((data: string) => {
    try {
      const msg: WsMessage = JSON.parse(data);
      if (msg.event === 'create' || msg.event === 'change' || msg.event === 'remove') {
        const parent = parentDirectory(msg.path);
        if (getDirectoryLoaded(parent)) scheduleTreeRefresh(parent);
      }
    } catch {
      // Ignore malformed watcher payloads.
    }
  });
}

void loadDirectory();
initWs();
if (initialFile) openFile(initialFile);
