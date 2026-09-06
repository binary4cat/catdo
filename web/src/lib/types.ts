export interface FileNode {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified: number;
  children?: FileNode[];
}

export interface FileContent {
  path: string;
  content: string;
  modified: number;
  version: string;
}

export interface VaultConfig {
  version: number;
  assetsDirName: string;
  theme: string;
  autoSaveIntervalMs: number;
  wikiLinks: {
    autoCreate: boolean;
  };
}

export interface TabItem {
  path: string;
  name: string;
  modified: boolean;
}

export interface WsMessage {
  event: 'change' | 'create' | 'remove';
  path: string;
}
