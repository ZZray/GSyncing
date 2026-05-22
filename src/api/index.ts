import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  BackendConfig,
  GameProfile,
  GameSizeInfo,
  GameSyncStatus,
  LogEntry,
  SnapshotSummary,
  StatEntry,
  SyncPreview,
  SyncProgress,
  VersionInfo,
} from "@/types";

export async function bootstrap(): Promise<void> {
  await invoke("bootstrap");
}

export async function listGames(): Promise<GameProfile[]> {
  return invoke("list_games");
}

export async function saveGame(game: GameProfile): Promise<GameProfile> {
  return invoke("save_game", { game });
}

export async function deleteGame(id: string): Promise<void> {
  await invoke("delete_game", { id });
}

export async function listBackends(): Promise<BackendConfig[]> {
  return invoke("list_backends");
}

export async function saveBackend(
  backend: BackendConfig
): Promise<BackendConfig[]> {
  return invoke("save_backend", { backend });
}

export async function deleteBackend(name: string): Promise<BackendConfig[]> {
  return invoke("delete_backend", { name });
}

export async function testBackend(backend: BackendConfig): Promise<string> {
  return invoke("test_backend", { backend });
}

export async function getSettings(): Promise<AppSettings> {
  return invoke("get_settings");
}

export async function saveSettings(settings: AppSettings): Promise<AppSettings> {
  return invoke("save_settings", { settings });
}

export async function listStatus(): Promise<GameSyncStatus[]> {
  return invoke("list_status");
}

export async function syncOne(gameId: string, direction: "auto" | "push" | "pull"): Promise<void> {
  await invoke("sync_one", { gameId, direction });
}

export async function syncAll(): Promise<void> {
  await invoke("sync_all");
}

export async function syncPreview(
  gameId: string,
  direction: "auto" | "push" | "pull"
): Promise<SyncPreview> {
  return invoke("sync_preview", { gameId, direction });
}

export async function syncWithOverrides(
  gameId: string,
  direction: "auto" | "push" | "pull",
  overrides: Record<string, "local" | "remote" | "rename">
): Promise<void> {
  await invoke("sync_with_overrides", { gameId, direction, overrides });
}

export async function listVersions(gameId: string): Promise<VersionInfo[]> {
  return invoke("list_versions", { gameId });
}

export async function restoreVersion(
  gameId: string,
  versionKey: string
): Promise<void> {
  await invoke("restore_version", { gameId, versionKey });
}

export async function cancelSync(gameId: string): Promise<boolean> {
  return invoke("cancel_sync", { gameId });
}

export async function createSnapshot(
  gameId: string,
  name: string
): Promise<SnapshotSummary> {
  return invoke("create_snapshot", { gameId, name });
}

export async function listSnapshots(
  gameId: string
): Promise<SnapshotSummary[]> {
  return invoke("list_snapshots", { gameId });
}

export async function restoreSnapshot(
  gameId: string,
  snapshotId: string
): Promise<void> {
  await invoke("restore_snapshot", { gameId, snapshotId });
}

export async function deleteSnapshot(
  gameId: string,
  snapshotId: string
): Promise<void> {
  await invoke("delete_snapshot", { gameId, snapshotId });
}

export async function deleteVersion(
  gameId: string,
  versionKey: string
): Promise<void> {
  await invoke("delete_version", { gameId, versionKey });
}

export async function exportVersion(
  gameId: string,
  versionKey: string,
  localPath: string
): Promise<void> {
  await invoke("export_version", { gameId, versionKey, localPath });
}

export async function exportConfig(
  path: string,
  includeSecrets: boolean
): Promise<string> {
  return invoke("export_config", { path, includeSecrets });
}

export async function importConfig(
  path: string,
  mode: "merge" | "replace"
): Promise<void> {
  await invoke("import_config", { path, mode });
}

export async function readLog(limit = 500): Promise<LogEntry[]> {
  return invoke("read_log", { limit });
}

export async function readStats(): Promise<StatEntry[]> {
  return invoke("read_stats");
}

export async function flushLog(): Promise<string> {
  return invoke("flush_log");
}

export async function getDataDir(): Promise<string> {
  return invoke("get_data_dir");
}

export async function computeSaveSize(gameId: string): Promise<GameSizeInfo> {
  return invoke("compute_save_size", { gameId });
}

export async function validateGamePaths(
  game: GameProfile
): Promise<GameSizeInfo> {
  return invoke("validate_game_paths", { game });
}

export type StatusEvent = { gameId: string; status: GameSyncStatus };
export type GlobalSyncEvent = {
  running: boolean;
  message?: string;
  lastSuccessAt?: number;
  lastError?: string;
};

export function onStatusChange(
  cb: (e: StatusEvent) => void
): Promise<UnlistenFn> {
  return listen<StatusEvent>("status-change", (evt) => cb(evt.payload));
}

export function onGlobalSync(
  cb: (e: GlobalSyncEvent) => void
): Promise<UnlistenFn> {
  return listen<GlobalSyncEvent>("global-sync", (evt) => cb(evt.payload));
}

export function onLogEntry(cb: (e: LogEntry) => void): Promise<UnlistenFn> {
  return listen<LogEntry>("log-entry", (evt) => cb(evt.payload));
}

export function onSyncProgress(
  cb: (e: SyncProgress) => void
): Promise<UnlistenFn> {
  return listen<SyncProgress>("sync-progress", (evt) => cb(evt.payload));
}
