import type { FileNode, FileContent, VaultConfig } from './types';
import type { PatchOperation } from './line-diff';

const BASE = '';

export async function fetchTree(path = ''): Promise<FileNode[]> {
  const query = path ? `?path=${encodeURIComponent(path)}` : '';
  const res = await fetch(`${BASE}/api/tree${query}`);
  if (!res.ok) throw new Error('Failed to fetch tree');
  return res.json();
}

export async function searchFiles(query: string): Promise<FileNode[]> {
  const res = await fetch(`${BASE}/api/search?query=${encodeURIComponent(query)}`);
  if (!res.ok) throw new Error('Failed to search files');
  return res.json();
}

export async function fetchMarkdownPaths(): Promise<string[]> {
  const res = await fetch(`${BASE}/api/markdown-paths`);
  if (!res.ok) throw new Error('Failed to fetch markdown paths');
  return res.json();
}

export async function fetchFile(path: string): Promise<FileContent> {
  const res = await fetch(`${BASE}/api/file?path=${encodeURIComponent(path)}`);
  if (!res.ok) throw new Error(`Failed to fetch file: ${path}`);
  return res.json();
}

export interface SavedFile {
  ok: boolean;
  path: string;
  version: string;
  replayed?: boolean;
}

export async function saveFile(
  path: string,
  content: string,
  expectedVersion?: string,
  requestId?: string,
): Promise<SavedFile> {
  const body: {
    path: string;
    content: string;
    expected_version?: string;
    request_id?: string;
  } = { path, content };
  if (expectedVersion !== undefined) body.expected_version = expectedVersion;
  if (requestId !== undefined) body.request_id = requestId;
  const res = await fetch(`${BASE}/api/file`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    let detail: Record<string, unknown> = {};
    try {
      detail = await res.json();
    } catch {
      // ignore parse failure
    }
    const error = new Error(`Failed to save file: ${path}`) as Error & {
      status?: number;
      detail?: Record<string, unknown>;
    };
    error.status = res.status;
    error.detail = detail;
    throw error;
  }
  return res.json();
}

// --- PATCH API ---

export interface PatchResult {
  ok: boolean;
  path: string;
  version: string;
  applied_operations: number;
  replayed?: boolean;
}

export interface PatchError {
  error: string;
  current_version?: string;
  failed_operation?: {
    index: number;
    reason: string;
  };
}

export async function patchFile(
  path: string,
  baseVersion: string,
  operations: PatchOperation[],
  requestId?: string,
): Promise<PatchResult> {
  const body: {
    path: string;
    base_version: string;
    operations: PatchOperation[];
    request_id?: string;
  } = { path, base_version: baseVersion, operations };
  if (requestId !== undefined) body.request_id = requestId;
  const res = await fetch(`${BASE}/api/file`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    let detail: PatchError = { error: 'unknown' };
    try {
      detail = await res.json();
    } catch {
      // ignore parse failure
    }
    const error = new Error(`Failed to patch file: ${path}`) as Error & {
      status?: number;
      detail?: PatchError;
    };
    error.status = res.status;
    error.detail = detail;
    throw error;
  }
  return res.json();
}

export async function uploadAsset(
  targetDir: string,
  file: File,
  fileName: string,
): Promise<string> {
  const formData = new FormData();
  formData.append('target_dir', targetDir);
  formData.append('file', file, fileName);
  const res = await fetch(`${BASE}/api/assets/upload`, {
    method: 'POST',
    body: formData,
  });
  if (!res.ok) {
    const error = new Error('Failed to upload asset') as Error & { status?: number };
    error.status = res.status;
    throw error;
  }
  const data = await res.json();
  return data.relative_path;
}

export async function fetchConfig(): Promise<VaultConfig> {
  const res = await fetch(`${BASE}/api/config`);
  if (!res.ok) throw new Error('Failed to fetch config');
  return res.json();
}

export async function saveConfig(config: VaultConfig): Promise<void> {
  const res = await fetch(`${BASE}/api/config`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(config),
  });
  if (!res.ok) throw new Error('Failed to save config');
}

export function rawUrl(path: string): string {
  return `${BASE}/raw/${path}`;
}

export function connectWs(onMessage: (data: string) => void): WebSocket {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  const ws = new WebSocket(`${proto}//${location.host}/api/ws`);
  ws.onmessage = (e) => onMessage(e.data);
  ws.onclose = () => {
    // Reconnect after 2s
    setTimeout(() => connectWs(onMessage), 2000);
  };
  return ws;
}
