import { useState } from "react";
import { Button, Modal, Progress, Space, App as AntdApp, Tag } from "antd";
import { DownloadOutlined, CheckCircleOutlined } from "@ant-design/icons";
import { check, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { errMsg } from "@/api/err";

interface Props {
  /** Button text — "检查更新" by default. */
  label?: string;
  /** Render style. */
  type?: "default" | "primary" | "link" | "text" | "dashed";
  size?: "small" | "middle" | "large";
}

/**
 * Self-contained UpdateChecker. Calls `check()` from the updater plugin,
 * shows a modal with release notes + download progress, and relaunches the
 * app after the new MSI is applied.
 *
 * Failure modes are common and handled inline:
 *  - endpoint unreachable (offline / DNS error) → toast + ignored
 *  - signature mismatch → fatal toast
 *  - no update available → success toast
 */
export default function UpdateChecker({
  label = "检查更新",
  type = "default",
  size = "middle",
}: Props) {
  const [checking, setChecking] = useState(false);
  const [update, setUpdate] = useState<Update | null>(null);
  const [progress, setProgress] = useState<{
    downloaded: number;
    total: number;
  } | null>(null);
  const [installing, setInstalling] = useState(false);
  const { message } = AntdApp.useApp();

  const doCheck = async () => {
    setChecking(true);
    try {
      const u = await check();
      if (!u || !u.available) {
        message.success("已是最新版本");
        return;
      }
      setUpdate(u);
    } catch (e: unknown) {
      message.error(`检查失败：${errMsg(e)}`);
    } finally {
      setChecking(false);
    }
  };

  const doInstall = async () => {
    if (!update) return;
    setInstalling(true);
    try {
      let total = 0;
      let downloaded = 0;
      await update.downloadAndInstall((evt) => {
        switch (evt.event) {
          case "Started":
            total = evt.data?.contentLength ?? 0;
            setProgress({ downloaded: 0, total });
            break;
          case "Progress":
            downloaded += evt.data?.chunkLength ?? 0;
            setProgress({ downloaded, total });
            break;
          case "Finished":
            setProgress({ downloaded: total, total });
            break;
        }
      });
      message.success("安装完成，即将重启");
      // Give the toast 600ms to flash before the process tears down.
      setTimeout(() => {
        relaunch().catch((e) => message.error(`重启失败：${errMsg(e)}`));
      }, 600);
    } catch (e: unknown) {
      message.error(`下载/安装失败：${errMsg(e)}`);
      setInstalling(false);
    }
  };

  const pct =
    progress && progress.total > 0
      ? Math.round((progress.downloaded / progress.total) * 100)
      : 0;

  return (
    <>
      <Button
        type={type}
        size={size}
        icon={<DownloadOutlined />}
        loading={checking}
        onClick={doCheck}
      >
        {label}
      </Button>

      <Modal
        open={!!update}
        title={
          update ? (
            <Space>
              发现新版本
              <Tag color="blue">v{update.version}</Tag>
            </Space>
          ) : (
            "更新"
          )
        }
        onCancel={() => !installing && setUpdate(null)}
        footer={null}
        maskClosable={!installing}
        closable={!installing}
        width={520}
      >
        {update && (
          <>
            <div style={{ marginBottom: 12, color: "#666", fontSize: 13 }}>
              发布时间：{update.date ?? "—"}
            </div>
            {update.body && (
              <pre
                style={{
                  background: "var(--bg-panel-soft, #f5f7fb)",
                  padding: 12,
                  borderRadius: 8,
                  fontSize: 12,
                  maxHeight: 260,
                  overflow: "auto",
                  whiteSpace: "pre-wrap",
                  margin: "0 0 14px",
                }}
              >
                {update.body}
              </pre>
            )}

            {progress && (
              <div style={{ marginBottom: 14 }}>
                <Progress
                  percent={pct}
                  status={pct >= 100 ? "success" : "active"}
                  format={(p) =>
                    `${p}% · ${humanBytes(progress.downloaded)} / ${humanBytes(
                      progress.total
                    )}`
                  }
                />
              </div>
            )}

            <Space style={{ width: "100%", justifyContent: "flex-end" }}>
              <Button
                disabled={installing}
                onClick={() => setUpdate(null)}
              >
                稍后再说
              </Button>
              <Button
                type="primary"
                icon={pct >= 100 ? <CheckCircleOutlined /> : <DownloadOutlined />}
                loading={installing && pct < 100}
                onClick={doInstall}
                disabled={installing}
              >
                {pct >= 100
                  ? "重启应用"
                  : installing
                  ? "下载中..."
                  : "立即下载并安装"}
              </Button>
            </Space>
          </>
        )}
      </Modal>
    </>
  );
}

function humanBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}
