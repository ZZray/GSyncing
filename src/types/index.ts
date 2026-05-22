export type BackendKind = "s3" | "webdav";

export interface S3Config {
  /** S3-compatible endpoint, e.g. https://cos.ap-shanghai.myqcloud.com */
  endpoint: string;
  region: string;
  bucket: string;
  accessKeyId: string;
  secretAccessKey: string;
  /** key prefix inside the bucket */
  prefix: string;
  /** force path-style addressing — required for many non-AWS providers */
  pathStyle: boolean;
}

export interface WebDAVConfig {
  url: string;
  username: string;
  password: string;
  /** sub-path inside the WebDAV root */
  prefix: string;
}

export type BackendConfig =
  | { kind: "s3"; name: string; s3: S3Config }
  | { kind: "webdav"; name: string; webdav: WebDAVConfig };

export interface GameProfile {
  id: string;
  name: string;
  /** root directories to sync — supports globs */
  savePaths: string[];
  /** glob patterns to include (defaults to **\/*) */
  include: string[];
  /** glob patterns to exclude */
  exclude: string[];
  /** key prefix inside the backend (defaults to game id) */
  remotePrefix?: string;
  /** whether to auto-sync this game */
  autoSync: boolean;
  /** process name for "sync on exit" trigger (Windows) */
  processName?: string;
  /** icon emoji or short label rendered in the card cover */
  cover?: string;
  /** per-game backend override; falls back to default if missing/unknown */
  backend?: string;
  /** optional category for visual grouping ("RPG", "Action", ...) */
  category?: string;
  /** user-pinned games always sort to the top of the library */
  pinned?: boolean;
}

export type SyncState =
  | "idle"
  | "scanning"
  | "uploading"
  | "downloading"
  | "synced"
  | "dirty"
  | "error";

export interface GameSyncStatus {
  gameId: string;
  state: SyncState;
  message?: string;
  localFiles: number;
  remoteFiles: number;
  lastSyncAt?: number;
  lastError?: string;
}

export interface AppSettings {
  defaultBackend?: string;
  autoSyncIntervalSec: number;
  enableFileWatcher: boolean;
  enableExitSync: boolean;
  conflictPolicy:
    | "rename-both"
    | "newer-wins"
    | "ask"
    | "local-wins"
    | "remote-wins";
  maxConcurrency: number;
  versionsToKeep: number;
  alwaysPreview: boolean;
  closeToTray: boolean;
  maxBytesPerSec: number;
  notifyOnComplete: boolean;
  autoOpenDevtools: boolean;
  theme: "light" | "dark" | "auto";
  autoCheckUpdates: boolean;
}

export interface PreviewItem {
  rel: string;
  size: number;
}

export interface PreviewConflict {
  rel: string;
  localSize: number;
  localMtime: number;
  remoteSize: number;
  remoteMtime: number;
}

export interface SyncPreview {
  gameId: string;
  direction: "auto" | "push" | "pull";
  uploads: PreviewItem[];
  downloads: PreviewItem[];
  deleteRemote: string[];
  deleteLocal: string[];
  conflicts: PreviewConflict[];
  totalBytes: number;
}

export interface VersionInfo {
  rel: string;
  key: string;
  timestampMs: number;
  size: number;
}

export interface SnapshotSummary {
  id: string;
  name: string;
  createdAt: number;
  fileCount: number;
  totalSize: number;
}

export interface GameSizeInfo {
  fileCount: number;
  totalBytes: number;
}

export interface StatEntry {
  ts: number;
  gameId: string;
  direction: string;
  success: boolean;
  uploadedFiles: number;
  downloadedFiles: number;
  totalBytes: number;
  durationMs: number;
  error?: string;
}

export interface SyncProgress {
  gameId: string;
  phase: "upload" | "download" | string;
  current: number;
  total: number;
  currentFile?: string;
  bytesDone: number;
  bytesTotal: number;
  /** unix-ms when this transfer started — frontend uses now-startedAt for speed */
  startedAt: number;
  /** server-side now() — pairs with startedAt to compute speed independently of clock skew */
  now: number;
}

export interface LogEntry {
  ts: number;
  level: "info" | "warn" | "error" | "debug";
  scope: string;
  message: string;
}
