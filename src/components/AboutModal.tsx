import { useEffect, useState } from "react";
import { Modal, Descriptions, Button, Space, Tag, App as AntdApp } from "antd";
import { FolderOpenOutlined, GithubOutlined } from "@ant-design/icons";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { getDataDir } from "@/api";
import { errMsg } from "@/api/err";
import UpdateChecker from "./UpdateChecker";

interface Props {
  open: boolean;
  onClose: () => void;
}

export const APP_VERSION = "0.1.0";
export const APP_NAME = "GSyncing";

export default function AboutModal({ open, onClose }: Props) {
  const [dataDir, setDataDir] = useState<string>("(loading...)");
  const { message } = AntdApp.useApp();

  useEffect(() => {
    if (!open) return;
    getDataDir()
      .then(setDataDir)
      .catch((e) => setDataDir(`读取失败：${errMsg(e)}`));
  }, [open]);

  const openDataDir = async () => {
    try {
      await openPath(dataDir);
    } catch (e) {
      message.error(`打开失败：${errMsg(e)}`);
    }
  };

  const openGithub = async () => {
    try {
      await openUrl("https://github.com/ZZray/GSyncing");
    } catch (e) {
      message.error(`打开失败：${errMsg(e)}`);
    }
  };

  return (
    <Modal
      open={open}
      onCancel={onClose}
      onOk={onClose}
      title="关于 GSyncing"
      width={520}
      footer={
        <Space>
          <UpdateChecker />
          <Button icon={<GithubOutlined />} onClick={openGithub}>
            GitHub
          </Button>
          <Button icon={<FolderOpenOutlined />} onClick={openDataDir}>
            打开数据目录
          </Button>
          <Button type="primary" onClick={onClose}>
            关闭
          </Button>
        </Space>
      }
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 16,
          marginBottom: 18,
        }}
      >
        <div
          style={{
            width: 56,
            height: 56,
            borderRadius: 14,
            background: "linear-gradient(135deg, #5b8def 0%, #8a6cff 100%)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: "#fff",
            fontWeight: 700,
            fontSize: 30,
            boxShadow: "0 6px 18px rgba(91, 141, 239, 0.35)",
          }}
        >
          G
        </div>
        <div>
          <div style={{ fontSize: 18, fontWeight: 600 }}>{APP_NAME}</div>
          <div style={{ color: "#888", fontSize: 12 }}>
            游戏存档云同步 · Tauri 2 + Rust + React + Antd
          </div>
        </div>
      </div>

      <Descriptions size="small" column={1} bordered>
        <Descriptions.Item label="版本">
          <Tag color="blue">v{APP_VERSION}</Tag>
        </Descriptions.Item>
        <Descriptions.Item label="License">MIT</Descriptions.Item>
        <Descriptions.Item label="后端">
          S3 兼容（COS / OSS / AWS）+ WebDAV
        </Descriptions.Item>
        <Descriptions.Item label="数据目录">
          <code style={{ fontSize: 11, wordBreak: "break-all" }}>{dataDir}</code>
        </Descriptions.Item>
      </Descriptions>

      <div style={{ marginTop: 14, color: "#888", fontSize: 12 }}>
        本程序不主动上传任何遥测数据。所有游戏存档存于你自己配置的云存储。
        Windows 上凭据用 DPAPI 加密绑定到当前用户。
      </div>
    </Modal>
  );
}
