import { useEffect, useState } from "react";
import {
  Modal,
  Tabs,
  Tag,
  Table,
  Empty,
  Spin,
  Alert,
  App as AntdApp,
  Radio,
} from "antd";
import {
  CloudUploadOutlined,
  CloudDownloadOutlined,
  DeleteOutlined,
  ExclamationCircleOutlined,
} from "@ant-design/icons";
import type { SyncPreview, PreviewConflict } from "@/types";
import { syncPreview, syncOne, syncWithOverrides } from "@/api";
import { errMsg } from "@/api/err";
import { useAppStore } from "@/store";

interface Props {
  open: boolean;
  gameId: string;
  gameName: string;
  direction: "auto" | "push" | "pull";
  onClose: () => void;
}

function humanBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

type ConflictChoice = "local" | "remote" | "rename";

export default function SyncPreviewModal({
  open,
  gameId,
  gameName,
  direction,
  onClose,
}: Props) {
  const [loading, setLoading] = useState(false);
  const [preview, setPreview] = useState<SyncPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [executing, setExecuting] = useState(false);
  const [conflictChoices, setConflictChoices] = useState<
    Record<string, ConflictChoice>
  >({});
  const policy = useAppStore((s) => s.settings?.conflictPolicy ?? "rename-both");
  const { message } = AntdApp.useApp();

  // Default each conflict to "rename" (保留双方 = the safe default) whenever
  // the preview refreshes. User can change per-row.
  useEffect(() => {
    if (!preview) return;
    setConflictChoices((prev) => {
      const next = { ...prev };
      for (const c of preview.conflicts) {
        if (!next[c.rel]) next[c.rel] = "rename";
      }
      return next;
    });
  }, [preview]);

  const askMode = policy === "ask" && (preview?.conflicts.length ?? 0) > 0;

  useEffect(() => {
    if (!open) {
      setPreview(null);
      setError(null);
      return;
    }
    setLoading(true);
    setError(null);
    syncPreview(gameId, direction)
      .then((p) => setPreview(p))
      .catch((e) => setError(errMsg(e)))
      .finally(() => setLoading(false));
  }, [open, gameId, direction]);

  const counts = preview
    ? {
        uploads: preview.uploads.length,
        downloads: preview.downloads.length,
        deleteR: preview.deleteRemote.length,
        deleteL: preview.deleteLocal.length,
        conflicts: preview.conflicts.length,
      }
    : null;

  const hasWork =
    preview &&
    (preview.uploads.length +
      preview.downloads.length +
      preview.deleteRemote.length +
      preview.deleteLocal.length +
      preview.conflicts.length) >
      0;

  const handleConfirm = async () => {
    setExecuting(true);
    try {
      if (askMode) {
        await syncWithOverrides(gameId, direction, conflictChoices);
        message.success(
          `已触发 ${gameName} 的同步（应用 ${
            Object.keys(conflictChoices).length
          } 项冲突选择）`
        );
      } else {
        await syncOne(gameId, direction);
        message.success(`已触发 ${gameName} 的同步`);
      }
      onClose();
    } catch (e: unknown) {
      message.error(`同步失败：${errMsg(e)}`);
    } finally {
      setExecuting(false);
    }
  };

  return (
    <Modal
      open={open}
      onCancel={onClose}
      onOk={handleConfirm}
      okText={hasWork ? "执行同步" : "无变化"}
      cancelText="取消"
      width={720}
      okButtonProps={{ disabled: !hasWork || !!error, loading: executing }}
      title={
        <span>
          同步预览 · {gameName}{" "}
          <Tag color="blue" style={{ marginLeft: 8 }}>
            {direction === "auto"
              ? "双向"
              : direction === "push"
              ? "推送 (本地→远端)"
              : "拉取 (远端→本地)"}
          </Tag>
        </span>
      }
    >
      {loading && (
        <div style={{ textAlign: "center", padding: 40 }}>
          <Spin tip="扫描中..." />
        </div>
      )}
      {error && <Alert type="error" message={error} showIcon />}
      {!loading && !error && preview && (
        <>
          <Alert
            type={hasWork ? "info" : "success"}
            showIcon
            style={{ marginBottom: 14 }}
            message={
              hasWork ? (
                <span>
                  本次同步将处理{" "}
                  <b>
                    {(counts?.uploads || 0) +
                      (counts?.downloads || 0) +
                      (counts?.deleteR || 0) +
                      (counts?.deleteL || 0) +
                      (counts?.conflicts || 0)}
                  </b>{" "}
                  个文件，传输约 <b>{humanBytes(preview.totalBytes)}</b>
                  {counts?.conflicts ? (
                    <span style={{ color: "#d4380d", marginLeft: 8 }}>
                      · 检测到 {counts.conflicts} 处冲突（将保留双方）
                    </span>
                  ) : null}
                </span>
              ) : (
                <span>本地与远端已经一致，无需同步</span>
              )
            }
          />
          <Tabs
            items={[
              {
                key: "up",
                label: (
                  <span>
                    <CloudUploadOutlined /> 上传 ({counts?.uploads})
                  </span>
                ),
                children: <FileList items={preview.uploads} />,
              },
              {
                key: "down",
                label: (
                  <span>
                    <CloudDownloadOutlined /> 下载 ({counts?.downloads})
                  </span>
                ),
                children: <FileList items={preview.downloads} />,
              },
              {
                key: "delR",
                label: (
                  <span>
                    <DeleteOutlined /> 删除远端 ({counts?.deleteR})
                  </span>
                ),
                children: <PathList items={preview.deleteRemote} />,
              },
              {
                key: "delL",
                label: (
                  <span>
                    <DeleteOutlined /> 删除本地 ({counts?.deleteL})
                  </span>
                ),
                children: <PathList items={preview.deleteLocal} />,
              },
              {
                key: "conf",
                label: (
                  <span>
                    <ExclamationCircleOutlined />{" "}
                    冲突 ({counts?.conflicts})
                  </span>
                ),
                children: (
                  <ConflictList
                    items={preview.conflicts}
                    askMode={askMode}
                    choices={conflictChoices}
                    onChoiceChange={(rel, c) =>
                      setConflictChoices((prev) => ({ ...prev, [rel]: c }))
                    }
                    onBulkApply={(c) => {
                      setConflictChoices(() => {
                        const next: Record<string, ConflictChoice> = {};
                        for (const conf of preview.conflicts) {
                          next[conf.rel] = c;
                        }
                        return next;
                      });
                    }}
                  />
                ),
              },
            ]}
          />
        </>
      )}
    </Modal>
  );
}

function FileList({ items }: { items: { rel: string; size: number }[] }) {
  if (items.length === 0) return <Empty description="无" />;
  return (
    <Table
      size="small"
      pagination={items.length > 20 ? { pageSize: 20 } : false}
      dataSource={items.map((i, idx) => ({ ...i, key: idx }))}
      columns={[
        { title: "路径", dataIndex: "rel" },
        {
          title: "大小",
          dataIndex: "size",
          width: 110,
          render: (s: number) => humanBytes(s),
        },
      ]}
    />
  );
}

function PathList({ items }: { items: string[] }) {
  if (items.length === 0) return <Empty description="无" />;
  return (
    <Table
      size="small"
      pagination={items.length > 20 ? { pageSize: 20 } : false}
      dataSource={items.map((rel, idx) => ({ rel, key: idx }))}
      columns={[{ title: "路径", dataIndex: "rel" }]}
    />
  );
}

function ConflictList({
  items,
  askMode,
  choices,
  onChoiceChange,
  onBulkApply,
}: {
  items: PreviewConflict[];
  askMode: boolean;
  choices: Record<string, ConflictChoice>;
  onChoiceChange: (rel: string, choice: ConflictChoice) => void;
  onBulkApply: (choice: ConflictChoice) => void;
}) {
  if (items.length === 0) return <Empty description="无冲突" />;
  return (
    <>
      {askMode && (
        <>
          <Alert
            type="warning"
            showIcon
            style={{ marginBottom: 10 }}
            message="冲突策略 = 弹窗确认"
            description="为每个冲突文件挑选处理方式。默认「保留双方」最安全，但会留下两份重命名副本。"
          />
          <div
            style={{
              marginBottom: 10,
              display: "flex",
              alignItems: "center",
              gap: 8,
              fontSize: 12,
              color: "var(--fg-muted, #666)",
            }}
          >
            <span>批量应用：</span>
            <Radio.Group
              size="small"
              onChange={(e) => onBulkApply(e.target.value)}
              value={null}
            >
              <Radio.Button value="local">全部保留本地</Radio.Button>
              <Radio.Button value="remote">全部保留远端</Radio.Button>
              <Radio.Button value="rename">全部保留双方</Radio.Button>
            </Radio.Group>
          </div>
        </>
      )}
      <Table
        size="small"
        pagination={items.length > 20 ? { pageSize: 20 } : false}
        dataSource={items.map((c, idx) => ({ ...c, key: idx }))}
        columns={[
          { title: "路径", dataIndex: "rel", width: 260 },
          {
            title: "本地",
            dataIndex: "localSize",
            width: 200,
            render: (_: unknown, r) => (
              <span style={{ fontSize: 12 }}>
                {humanBytes(r.localSize)}
                <br />
                <span style={{ color: "#888" }}>
                  {new Date(r.localMtime).toLocaleString()}
                </span>
              </span>
            ),
          },
          ...(askMode
            ? [
                {
                  title: "处理方式",
                  dataIndex: "rel",
                  render: (rel: string) => (
                    <Radio.Group
                      size="small"
                      value={choices[rel] ?? "rename"}
                      onChange={(e) => onChoiceChange(rel, e.target.value)}
                    >
                      <Radio.Button value="local">保留本地</Radio.Button>
                      <Radio.Button value="remote">保留远端</Radio.Button>
                      <Radio.Button value="rename">保留双方</Radio.Button>
                    </Radio.Group>
                  ),
                },
              ]
            : []),
        ]}
      />
    </>
  );
}
