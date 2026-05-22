import React, { useEffect, useMemo, useState } from "react";
import ReactDOM from "react-dom/client";
import { ConfigProvider, App as AntdApp, theme } from "antd";
import zhCN from "antd/locale/zh_CN";
import "dayjs/locale/zh-cn";
import App from "./App";
import { ErrorBoundary } from "./ErrorBoundary";
import { useAppStore } from "./store";
import "./styles.css";

window.addEventListener("error", (e) => {
  console.error("window.onerror:", e.error ?? e.message);
});
window.addEventListener("unhandledrejection", (e) => {
  console.error("unhandledrejection:", e.reason);
});

/**
 * Wires the live setting `theme` ("light" | "dark" | "auto") into Antd's
 * ConfigProvider algorithm. `auto` follows the OS via prefers-color-scheme.
 * Also toggles a `data-theme` attribute on <html> so plain-CSS styles can
 * branch via `[data-theme="dark"] .foo { ... }`.
 */
function ThemedApp() {
  const themeSetting = useAppStore((s) => s.settings?.theme ?? "light");
  const [systemDark, setSystemDark] = useState(() =>
    window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false
  );

  useEffect(() => {
    if (themeSetting !== "auto") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (e: MediaQueryListEvent) => setSystemDark(e.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [themeSetting]);

  const isDark =
    themeSetting === "dark" ||
    (themeSetting === "auto" && systemDark);

  useEffect(() => {
    document.documentElement.setAttribute(
      "data-theme",
      isDark ? "dark" : "light"
    );
  }, [isDark]);

  const algorithm = useMemo(
    () => (isDark ? theme.darkAlgorithm : theme.defaultAlgorithm),
    [isDark]
  );

  return (
    <ConfigProvider
      locale={zhCN}
      theme={{
        algorithm,
        token: {
          colorPrimary: "#5b8def",
          borderRadius: 8,
          fontFamily:
            '"Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "PingFang SC", "Microsoft YaHei", sans-serif',
        },
      }}
    >
      <AntdApp>
        <App />
      </AntdApp>
    </ConfigProvider>
  );
}

const rootEl = document.getElementById("root")!;

ReactDOM.createRoot(rootEl).render(
  <React.StrictMode>
    <ErrorBoundary>
      <ThemedApp />
    </ErrorBoundary>
  </React.StrictMode>
);
