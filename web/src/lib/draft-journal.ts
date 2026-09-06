// Crash-safe draft journal.
// IndexedDB is the durable async store; localStorage is a synchronous emergency
// shadow for the narrow window before an IndexedDB transaction commits.

const DB_NAME = 'catdo-drafts';
const DB_VERSION = 1;
const STORE_NAME = 'drafts';
const SHADOW_PREFIX = 'catdo:draft:';

export interface Draft {
  path: string;
  content: string;
  baseVersion: string;
  localRevision: number;
  updatedAt: number;
  saveState: 'dirty' | 'saving' | 'saved' | 'error';
}

let dbPromise: Promise<IDBDatabase> | null = null;
const writeChains = new Map<string, Promise<void>>();

function shadowKey(path: string): string {
  return `${SHADOW_PREFIX}${path}`;
}

function writeShadow(draft: Draft): void {
  try {
    localStorage.setItem(shadowKey(draft.path), JSON.stringify(draft));
  } catch {
    // Quota/security errors do not prevent the IndexedDB path.
  }
}

function readShadow(path: string): Draft | null {
  try {
    const raw = localStorage.getItem(shadowKey(path));
    return raw ? (JSON.parse(raw) as Draft) : null;
  } catch {
    return null;
  }
}

function clearShadow(path: string): void {
  try {
    localStorage.removeItem(shadowKey(path));
  } catch {
    // Ignore storage security errors.
  }
}

function openDB(): Promise<IDBDatabase> {
  if (dbPromise) return dbPromise;
  dbPromise = new Promise<IDBDatabase>((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME, { keyPath: 'path' });
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
  return dbPromise;
}

async function writeDraftNow(draft: Draft): Promise<void> {
  const db = await openDB();
  const tx = db.transaction(STORE_NAME, 'readwrite');
  tx.objectStore(STORE_NAME).put(draft);
  await new Promise<void>((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error ?? new Error('draft transaction aborted'));
  });
}

export function saveDraft(draft: Draft): Promise<void> {
  writeShadow(draft);
  const previous = writeChains.get(draft.path) ?? Promise.resolve();
  const next = previous.catch(() => undefined).then(() => writeDraftNow(draft));
  const tracked = next.finally(() => {
    if (writeChains.get(draft.path) === tracked) writeChains.delete(draft.path);
  });
  writeChains.set(draft.path, tracked);
  return tracked.catch((error) => {
    console.warn('[catdo] Failed to save draft:', error);
    throw error;
  });
}

export async function loadDraft(path: string): Promise<Draft | null> {
  const shadow = readShadow(path);
  try {
    const db = await openDB();
    const tx = db.transaction(STORE_NAME, 'readonly');
    const req = tx.objectStore(STORE_NAME).get(path);
    const indexed = await new Promise<Draft | null>((resolve, reject) => {
      req.onsuccess = () => resolve(req.result ?? null);
      req.onerror = () => reject(req.error);
    });
    if (!indexed) return shadow;
    if (!shadow) return indexed;
    return indexed.updatedAt >= shadow.updatedAt ? indexed : shadow;
  } catch (error) {
    console.warn('[catdo] Failed to load draft:', error);
    return shadow;
  }
}

export async function deleteDraft(path: string, revision?: number): Promise<void> {
  const previous = writeChains.get(path) ?? Promise.resolve();
  const next = previous
    .catch(() => undefined)
    .then(async () => {
      const shadow = readShadow(path);
      if (revision !== undefined && shadow && shadow.localRevision > revision) return;

      const db = await openDB();
      if (revision !== undefined) {
        const readTx = db.transaction(STORE_NAME, 'readonly');
        const current = await new Promise<Draft | null>((resolve, reject) => {
          const request = readTx.objectStore(STORE_NAME).get(path);
          request.onsuccess = () => resolve(request.result ?? null);
          request.onerror = () => reject(request.error);
        });
        if (current && current.localRevision > revision) return;
      }

      clearShadow(path);
      const tx = db.transaction(STORE_NAME, 'readwrite');
      tx.objectStore(STORE_NAME).delete(path);
      await new Promise<void>((resolve, reject) => {
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
        tx.onabort = () => reject(tx.error ?? new Error('draft delete aborted'));
      });
    });
  const tracked = next.finally(() => {
    if (writeChains.get(path) === tracked) writeChains.delete(path);
  });
  writeChains.set(path, tracked);
  await tracked;
}

export async function getAllDrafts(): Promise<Draft[]> {
  try {
    const db = await openDB();
    const tx = db.transaction(STORE_NAME, 'readonly');
    const req = tx.objectStore(STORE_NAME).getAll();
    return await new Promise<Draft[]>((resolve, reject) => {
      req.onsuccess = () => resolve(req.result ?? []);
      req.onerror = () => reject(req.error);
    });
  } catch (error) {
    console.warn('[catdo] Failed to list drafts:', error);
    return [];
  }
}

export async function recoverDrafts(): Promise<Draft[]> {
  const drafts = await getAllDrafts();
  return drafts.filter(
    (draft) => draft.saveState === 'dirty' || draft.saveState === 'saving' || draft.saveState === 'error',
  );
}

export async function requestPersistence(): Promise<boolean> {
  try {
    if (navigator.storage?.persist) return await navigator.storage.persist();
  } catch {
    // Not available.
  }
  return false;
}
