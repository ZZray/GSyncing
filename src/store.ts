import { create } from "zustand";
import type { UnlistenFn } from "@tauri-apps/api/event";
import * as api from "./api";
import { errMsg } from "./api/err";
import type {
  AppSettings,
  BackendConfig,
  GameProfile,
  GameSyncStatus,
  LogEntry,
  SyncProgress,
} from "./types";

interface GlobalSyncState {
  running: boolean;
  message?: string;
  lastSuccessAt?: number;
  lastError?: string;
}

interface AppState {
  games: GameProfile[];
  backends: BackendConfig[];
  settings: AppSettings | null;
  statusMap: Record<string, GameSyncStatus>;
  logs: LogEntry[];
  globalSync: GlobalSyncState;
  progress: SyncProgress | null;

  loadAll: () => Promise<void>;
  saveGame: (game: GameProfile) => Promise<void>;
  deleteGame: (id: string) => Promise<void>;
  saveBackend: (backend: BackendConfig) => Promise<void>;
  deleteBackend: (name: string) => Promise<void>;
  saveSettings: (settings: AppSettings) => Promise<void>;
  syncOne: (
    gameId: string,
    direction?: "auto" | "push" | "pull"
  ) => Promise<void>;
  syncAll: () => Promise<void>;
}

export const useAppStore = create<AppState>((set) => ({
  games: [],
  backends: [],
  settings: null,
  statusMap: {},
  logs: [],
  globalSync: { running: false },
  progress: null,

  async loadAll() {
    const [games, backends, settings, statusList, logs] = await Promise.all([
      api.listGames(),
      api.listBackends(),
      api.getSettings(),
      api.listStatus(),
      api.readLog(500),
    ]);
    const statusMap: Record<string, GameSyncStatus> = {};
    statusList.forEach((s) => {
      statusMap[s.gameId] = s;
    });
    set({ games, backends, settings, statusMap, logs });
  },

  async saveGame(game) {
    const saved = await api.saveGame(game);
    set((s) => {
      const idx = s.games.findIndex((g) => g.id === saved.id);
      const games = [...s.games];
      if (idx >= 0) games[idx] = saved;
      else games.push(saved);
      return { games };
    });
  },

  async deleteGame(id) {
    await api.deleteGame(id);
    set((s) => ({
      games: s.games.filter((g) => g.id !== id),
      statusMap: Object.fromEntries(
        Object.entries(s.statusMap).filter(([k]) => k !== id)
      ),
    }));
  },

  async saveBackend(backend) {
    const list = await api.saveBackend(backend);
    set({ backends: list });
  },

  async deleteBackend(name) {
    const list = await api.deleteBackend(name);
    set({ backends: list });
  },

  async saveSettings(settings) {
    const saved = await api.saveSettings(settings);
    set({ settings: saved });
  },

  async syncOne(gameId, direction = "auto") {
    await api.syncOne(gameId, direction);
  },

  async syncAll() {
    await api.syncAll();
  },
}));

/**
 * Subscribe to Tauri events ONCE at the top level — call this from main.tsx
 * after the store is constructed. Returns an unlisten function (used by tests
 * or HMR teardown; safe to ignore in production).
 */
export async function attachBackendListeners(): Promise<UnlistenFn[]> {
  const set = useAppStore.setState;
  const unlistens = await Promise.all([
    api.onStatusChange((evt) => {
      useAppStore.setState((s) => ({
        statusMap: { ...s.statusMap, [evt.gameId]: evt.status },
      }));
    }),
    api.onGlobalSync((evt) => {
      set({ globalSync: evt });
    }),
    api.onLogEntry((entry) => {
      useAppStore.setState((s) => ({
        logs: [...s.logs.slice(-499), entry],
      }));
    }),
    api.onSyncProgress((p) => {
      // Clear the progress bar when:
      //   - backend emits total=0 (explicit "done" sentinel after run_one), or
      //   - all bytes/files complete (current == total).
      if (p.total === 0 || (p.total > 0 && p.current >= p.total)) {
        set({ progress: null });
      } else {
        set({ progress: p });
      }
    }),
  ]);
  return unlistens;
}

export { errMsg };
