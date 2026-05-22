import { useEffect, useState } from "react";
import { Layout, Menu, Button, Tag, Space } from "antd";
import {
  AppstoreOutlined,
  CloudServerOutlined,
  SettingOutlined,
  FileSearchOutlined,
  SyncOutlined,
} from "@ant-design/icons";
import ProgressBar from "./components/ProgressBar";
import OnboardingWizard from "./components/OnboardingWizard";
import AboutModal, { APP_VERSION } from "./components/AboutModal";
import { check } from "@tauri-apps/plugin-updater";
import GameLibrary from "./pages/GameLibrary";
import SyncStatus from "./pages/SyncStatus";
import Settings from "./pages/Settings";
import LogViewer from "./pages/LogViewer";
import { useAppStore, attachBackendListeners } from "./store";
import { bootstrap } from "./api";

const { Sider } = Layout;

type PageKey = "library" | "status" | "settings" | "logs";

const menuItems = [
  { key: "library", icon: <AppstoreOutlined />, label: "游戏库" },
  { key: "status", icon: <CloudServerOutlined />, label: "同步状态" },
  { key: "settings", icon: <SettingOutlined />, label: "云存储设置" },
  { key: "logs", icon: <FileSearchOutlined />, label: "日志" },
];

export default function App() {
  const [page, setPage] = useState<PageKey>("library");
  const [onboardingOpen, setOnboardingOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const globalSync = useAppStore((s) => s.globalSync);
  const loadAll = useAppStore((s) => s.loadAll);
  const syncAll = useAppStore((s) => s.syncAll);
  const games = useAppStore((s) => s.games);
  const backends = useAppStore((s) => s.backends);
  const statusMap = useAppStore((s) => s.statusMap);

  // Global keyboard shortcuts. Keep the list small — power-user friendly
  // without surprising newcomers. All shortcuts use Ctrl (works as Cmd on
  // macOS via webview), and only fire when the user is NOT typing in a form.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      const target = e.target as HTMLElement | null;
      if (
        target?.tagName === "INPUT" ||
        target?.tagName === "TEXTAREA" ||
        target?.isContentEditable
      ) {
        return;
      }
      switch (e.key.toLowerCase()) {
        case "s": // Ctrl+S — sync all
          e.preventDefault();
          syncAll();
          break;
        case "l": // Ctrl+L — log page
          e.preventDefault();
          setPage("logs");
          break;
        case ",": // Ctrl+, — settings (mac-style)
          e.preventDefault();
          setPage("settings");
          break;
        case "/": // Ctrl+/ — about
          e.preventDefault();
          setAboutOpen(true);
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [syncAll]);

  useEffect(() => {
    let unlistens: Array<() => void> = [];
    // Each step swallows its own failure into the console (and ultimately
    // ErrorBoundary if rendering blows up). Without these `.catch`es a
    // rejection earlier in the chain leaves the user staring at an empty
    // shell with no diagnostic.
    bootstrap()
      .catch((e) => {
        console.error("bootstrap failed:", e);
      })
      .then(() => attachBackendListeners())
      .then((u) => {
        if (u) unlistens = u;
      })
      .catch((e) => {
        console.error("attach listeners failed:", e);
      })
      .then(() => loadAll())
      .catch((e) => {
        console.error("loadAll failed:", e);
      })
      .then(() => {
        const s = useAppStore.getState();
        if (s.backends.length === 0 && s.games.length === 0) {
          setOnboardingOpen(true);
        }
        // Silent update check on launch when the setting is on. Failure is
        // ignored — we don't want a flaky updater endpoint to spam errors.
        if (s.settings?.autoCheckUpdates) {
          check()
            .then((u) => {
              if (u?.available) {
                setAboutOpen(true);
              }
            })
            .catch((e) => console.warn("update check failed:", e));
        }
      })
      .catch((e) => {
        console.error("onboarding check failed:", e);
      });
    return () => {
      for (const u of unlistens) {
        try {
          u();
        } catch {
          /* noop */
        }
      }
    };
  }, [loadAll]);

  return (
    <div className="app-shell">
      <Sider className="app-sider" width={220} theme="dark">
        <div className="app-logo">
          <div className="app-logo-icon">G</div>
          <span>GSyncing</span>
        </div>
        <Menu
          mode="inline"
          theme="dark"
          selectedKeys={[page]}
          items={menuItems}
          onClick={({ key }) => setPage(key as PageKey)}
          style={{ flex: 1 }}
        />
        <div className="app-sider-footer">
          <span
            className="app-sider-version"
            onClick={() => setAboutOpen(true)}
            title="关于"
          >
            v{APP_VERSION} · 关于
          </span>
        </div>
      </Sider>
      <div className="app-main">
        <div className="app-header">
          <div style={{ display: "flex", alignItems: "center", gap: 18 }}>
            <div className="app-header-title">
              {menuItems.find((m) => m.key === page)?.label}
            </div>
            <HeaderStats
              games={games.length}
              backends={backends.length}
              lastSync={lastSyncTimestamp(statusMap)}
            />
          </div>
          <Space size="middle">
            <GlobalSyncIndicator />
            <Button
              type="primary"
              icon={<SyncOutlined spin={globalSync.running} />}
              loading={globalSync.running}
              onClick={() => syncAll()}
              title="一键同步 (Ctrl+S)"
            >
              一键同步
            </Button>
          </Space>
        </div>
        <ProgressBar />
        <OnboardingWizard
          open={onboardingOpen}
          onClose={() => setOnboardingOpen(false)}
        />
        <AboutModal open={aboutOpen} onClose={() => setAboutOpen(false)} />
        <div className="app-content">
          {page === "library" && <GameLibrary />}
          {page === "status" && <SyncStatus />}
          {page === "settings" && <Settings />}
          {page === "logs" && <LogViewer />}
        </div>
      </div>
    </div>
  );
}

function HeaderStats({
  games,
  backends,
  lastSync,
}: {
  games: number;
  backends: number;
  lastSync: number | null;
}) {
  return (
    <div className="app-header-stats">
      <span>
        <b>{games}</b> 游戏
      </span>
      <span className="sep">·</span>
      <span>
        <b>{backends}</b> 后端
      </span>
      {lastSync && (
        <>
          <span className="sep">·</span>
          <span>上次同步 {relativeTime(lastSync)}</span>
        </>
      )}
    </div>
  );
}

function lastSyncTimestamp(
  statusMap: Record<string, { lastSyncAt?: number }>
): number | null {
  let latest = 0;
  for (const s of Object.values(statusMap)) {
    if (s.lastSyncAt && s.lastSyncAt > latest) latest = s.lastSyncAt;
  }
  return latest > 0 ? latest : null;
}

function relativeTime(ms: number): string {
  const diff = (Date.now() - ms) / 1000;
  if (diff < 60) return "刚刚";
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  return `${Math.floor(diff / 86400)} 天前`;
}

function GlobalSyncIndicator() {
  const globalSync = useAppStore((s) => s.globalSync);
  if (globalSync.running) {
    return <Tag color="processing">{globalSync.message || "同步中..."}</Tag>;
  }
  if (globalSync.lastError) {
    return <Tag color="error">同步异常</Tag>;
  }
  if (globalSync.lastSuccessAt) {
    return (
      <Tag color="success">
        上次同步 {new Date(globalSync.lastSuccessAt).toLocaleTimeString()}
      </Tag>
    );
  }
  return <Tag>未同步</Tag>;
}
