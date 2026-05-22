import { useEffect, useMemo, useState } from "react";
import {
  Drawer,
  Button,
  Table,
  Empty,
  Spin,
  Alert,
  App as AntdApp,
  Tag,
  Input,
  Space,
  Popconfirm,
  Modal,
  Tabs,
  Select,
} from "antd";
import {
  ReloadOutlined,
  UndoOutlined,
  DeleteOutlined,
  PushpinOutlined,
  DownloadOutlined,
  ExclamationCircleOutlined,
} from "@ant-design/icons";
import type { GameProfile, VersionInfo, SnapshotSummary } from "@/types";
import {
  listVersions,
  restoreVersion,
  deleteVersion,
  exportVersion,
  listSnapshots,
  restoreSnapshot,
  deleteSnapshot,
} from "@/api";
import { errMsg } from "@/api/err";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";

interface Props {
  open: boolean;
  game: GameProfile;
  onClose: () => void;
}

function humanBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export default function VersionDrawer({ open, game, onClose }: Props) {
  const [tab, setTab] = useState<"snapshots" | "versions">("snapshots");

  return (
    <Drawer
      title={
        <Space>
          <span>历史版本 · {game.name}</span>
        </Space>
      }
      open={open}
      onClose={onClose}
      width={780}
    >
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 14 }}
        message={
          tab === "snapshots"
            ? "命名快照是你手动打的标签（如「黑暗剧情线-决战前」），永远不会被自动清理。适合多结局存档对比。"
            : "自动版本是每次同步覆盖前留下的安全网。按设置中的「保留历史版本数」滚动保留。"
        }
      />
      <Tabs
        activeKey={tab}
        onChange={(k) => setTab(k as "snapshots" | "versions")}
        items={[
          {
            key: "snapshots",
            label: (
              <span>
                <PushpinOutlined /> 命名快照
              </span>
            ),
            children: <SnapshotsTab game={game} />,
          },
          {
            key: "versions",
            label: <span>自动版本</span>,
            children: <VersionsTab game={game} />,
          },
        ]}
      />
    </Drawer>
  );
}

type SnapSort = "newest" | "oldest" | "files" | "size" | "name";

function SnapshotsTab({ game }: { game: GameProfile }) {
  const [loading, setLoading] = useState(false);
  const [snaps, setSnaps] = useState<SnapshotSummary[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [sort, setSort] = useState<SnapSort>("newest");
  const [busyId, setBusyId] = useState<string | null>(null);
  const { message } = AntdApp.useApp();

  const refresh = () => {
    setLoading(true);
    setError(null);
    listSnapshots(game.id)
      .then((s) => setSnaps(s))
      .catch((e) => setError(errMsg(e)))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [game.id]);

  const filtered = useMemo(() => {
    const q = filter.toLowerCase();
    const base = q
      ? snaps.filter((s) => s.name.toLowerCase().includes(q))
      : snaps.slice();
    switch (sort) {
      case "newest":
        return base.sort((a, b) => b.createdAt - a.createdAt);
      case "oldest":
        return base.sort((a, b) => a.createdAt - b.createdAt);
      case "files":
        return base.sort((a, b) => b.fileCount - a.fileCount);
      case "size":
        return base.sort((a, b) => b.totalSize - a.totalSize);
      case "name":
        return base.sort((a, b) => a.name.localeCompare(b.name, "zh"));
    }
    return base;
  }, [snaps, filter, sort]);

  const onRestore = (s: SnapshotSummary) => {
    Modal.confirm({
      title: `恢复到快照「${s.name}」？`,
      icon: <ExclamationCircleOutlined style={{ color: "#fa8c16" }} />,
      content: (
        <div style={{ fontSize: 13, lineHeight: 1.7 }}>
          <p style={{ marginBottom: 8 }}>
            会用此快照的 <b>{s.fileCount}</b> 个文件（共{" "}
            {humanBytes(s.totalSize)}）<b>覆盖</b>当前本地存档。
          </p>
          <p style={{ marginBottom: 0, color: "#888", fontSize: 12 }}>
            🔒 当前本地存档会自动归档进版本目录，恢复操作本身可逆 — 在「自动版本」标签里能找到刚刚被覆盖的版本回滚。
          </p>
        </div>
      ),
      okText: `恢复（${new Date(s.createdAt).toLocaleString()}）`,
      cancelText: "取消",
      onOk: async () => {
        setBusyId(s.id);
        try {
          await restoreSnapshot(game.id, s.id);
          message.success(`已恢复到快照「${s.name}」`);
        } catch (e: unknown) {
          message.error(`恢复失败：${errMsg(e)}`);
        } finally {
          setBusyId(null);
        }
      },
    });
  };

  const onDelete = async (s: SnapshotSummary) => {
    setBusyId(s.id);
    try {
      await deleteSnapshot(game.id, s.id);
      message.success("快照已删除");
      refresh();
    } catch (e: unknown) {
      message.error(`删除失败：${errMsg(e)}`);
    } finally {
      setBusyId(null);
    }
  };

  return (
    <>
      <Space style={{ marginBottom: 14 }}>
        <Input.Search
          placeholder="按名称筛选..."
          allowClear
          onSearch={setFilter}
          style={{ width: 240 }}
        />
        <Select
          value={sort}
          onChange={setSort}
          style={{ width: 140 }}
          options={[
            { label: "最新优先", value: "newest" },
            { label: "最早优先", value: "oldest" },
            { label: "文件多 → 少", value: "files" },
            { label: "体积大 → 小", value: "size" },
            { label: "名字 A-Z", value: "name" },
          ]}
        />
        <Button icon={<ReloadOutlined />} onClick={refresh} loading={loading}>
          刷新
        </Button>
      </Space>

      {loading ? (
        <div style={{ textAlign: "center", padding: 40 }}>
          <Spin tip="加载快照..." />
        </div>
      ) : error ? (
        <Alert type="error" message={error} showIcon />
      ) : filtered.length === 0 ? (
        <Empty
          description={
            snaps.length === 0
              ? "还没有命名快照 — 在游戏卡片菜单里点「标记当前为快照」开始"
              : "无匹配"
          }
        />
      ) : (
        <Table
          size="small"
          rowKey="id"
          pagination={false}
          dataSource={filtered}
          columns={[
            {
              title: "快照名称",
              dataIndex: "name",
              render: (n: string) => (
                <Space>
                  <PushpinOutlined style={{ color: "#8a6cff" }} />
                  <b>{n}</b>
                </Space>
              ),
            },
            {
              title: "创建时间",
              dataIndex: "createdAt",
              width: 180,
              render: (ts: number) => new Date(ts).toLocaleString(),
            },
            {
              title: "文件",
              dataIndex: "fileCount",
              width: 70,
            },
            {
              title: "大小",
              dataIndex: "totalSize",
              width: 100,
              render: (n: number) => humanBytes(n),
            },
            {
              title: "操作",
              width: 180,
              render: (_: unknown, s: SnapshotSummary) => (
                <Space>
                  <Button
                    size="small"
                    icon={<UndoOutlined />}
                    type="primary"
                    ghost
                    loading={busyId === s.id}
                    onClick={() => onRestore(s)}
                  >
                    恢复
                  </Button>
                  <Popconfirm
                    title={`删除快照「${s.name}」？`}
                    description="此快照的所有文件将从云端永久删除"
                    onConfirm={() => onDelete(s)}
                  >
                    <Button
                      size="small"
                      icon={<DeleteOutlined />}
                      danger
                      loading={busyId === s.id}
                    >
                      删除
                    </Button>
                  </Popconfirm>
                </Space>
              ),
            },
          ]}
        />
      )}
    </>
  );
}

function VersionsTab({ game }: { game: GameProfile }) {
  const [loading, setLoading] = useState(false);
  const [versions, setVersions] = useState<VersionInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [restoring, setRestoring] = useState<string | null>(null);
  const { message } = AntdApp.useApp();

  const refresh = () => {
    setLoading(true);
    setError(null);
    listVersions(game.id)
      .then((vs) => setVersions(vs))
      .catch((e) => setError(errMsg(e)))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [game.id]);

  const filtered = useMemo(() => {
    if (!filter.trim()) return versions;
    const q = filter.toLowerCase();
    return versions.filter((v) => v.rel.toLowerCase().includes(q));
  }, [versions, filter]);

  const groups = useMemo(() => {
    const byRel: Record<string, VersionInfo[]> = {};
    for (const v of filtered) {
      if (!byRel[v.rel]) byRel[v.rel] = [];
      byRel[v.rel].push(v);
    }
    return Object.entries(byRel)
      .map(([rel, items]) => ({
        key: rel,
        rel,
        count: items.length,
        items: items.sort((a, b) => b.timestampMs - a.timestampMs),
        newest: items[0],
      }))
      .sort((a, b) => b.newest.timestampMs - a.newest.timestampMs);
  }, [filtered]);

  const handleRestore = (v: VersionInfo) => {
    Modal.confirm({
      title: `恢复 ${v.rel}？`,
      icon: <ExclamationCircleOutlined style={{ color: "#fa8c16" }} />,
      content: (
        <div style={{ fontSize: 13, lineHeight: 1.7 }}>
          <p style={{ marginBottom: 8 }}>
            会用 <b>{new Date(v.timestampMs).toLocaleString()}</b> 的版本
            （{humanBytes(v.size)}）<b>覆盖</b>本地当前文件。
          </p>
          <p style={{ marginBottom: 0, color: "#888", fontSize: 12 }}>
            🔒 操作可逆：当前文件被覆盖前会自动再归档一份，能在「自动版本」标签里找到。
          </p>
        </div>
      ),
      okText: "恢复",
      cancelText: "取消",
      onOk: async () => {
        setRestoring(v.key);
        try {
          await restoreVersion(game.id, v.key);
          message.success(
            `已恢复 ${v.rel} 到 ${new Date(v.timestampMs).toLocaleString()}`
          );
          refresh();
        } catch (e: unknown) {
          message.error(`恢复失败：${errMsg(e)}`);
        } finally {
          setRestoring(null);
        }
      },
    });
  };

  const handleDelete = async (v: VersionInfo) => {
    setRestoring(v.key);
    try {
      await deleteVersion(game.id, v.key);
      message.success("版本已删除");
      refresh();
    } catch (e: unknown) {
      message.error(`删除失败：${errMsg(e)}`);
    } finally {
      setRestoring(null);
    }
  };

  const handleExport = async (v: VersionInfo) => {
    const base = v.rel.split("/").pop() || v.rel;
    const dst = await saveDialog({
      defaultPath: `${base}.${v.timestampMs}`,
      title: "导出版本到...",
    });
    if (!dst) return;
    setRestoring(v.key);
    try {
      await exportVersion(game.id, v.key, dst);
      message.success(`已导出到 ${dst}`);
    } catch (e: unknown) {
      message.error(`导出失败：${errMsg(e)}`);
    } finally {
      setRestoring(null);
    }
  };

  return (
    <>
      <Space style={{ marginBottom: 14 }}>
        <Input.Search
          placeholder="按路径筛选..."
          allowClear
          onSearch={setFilter}
          style={{ width: 260 }}
        />
        <Button icon={<ReloadOutlined />} onClick={refresh} loading={loading}>
          刷新
        </Button>
        <Tag color="purple">{versions.length} 个版本</Tag>
      </Space>

      {loading ? (
        <div style={{ textAlign: "center", padding: 40 }}>
          <Spin tip="加载版本列表..." />
        </div>
      ) : error ? (
        <Alert type="error" message={error} showIcon />
      ) : groups.length === 0 ? (
        <Empty
          description={
            versions.length === 0
              ? "还没有任何自动版本（首次同步后会逐步累积）"
              : "无匹配项"
          }
        />
      ) : (
        <Table
          size="small"
          dataSource={groups}
          pagination={false}
          expandable={{
            expandedRowRender: (row) => (
              <Table
                size="small"
                pagination={false}
                dataSource={row.items.map((it) => ({ ...it, key: it.key }))}
                columns={[
                  {
                    title: "时间",
                    dataIndex: "timestampMs",
                    render: (ts: number) => new Date(ts).toLocaleString(),
                  },
                  {
                    title: "大小",
                    dataIndex: "size",
                    width: 110,
                    render: (s: number) => humanBytes(s),
                  },
                  {
                    title: "操作",
                    width: 280,
                    render: (_: unknown, v) => (
                      <Space size={4}>
                        <Button
                          size="small"
                          icon={<UndoOutlined />}
                          loading={restoring === v.key}
                          type="primary"
                          ghost
                          onClick={() => handleRestore(v)}
                        >
                          恢复
                        </Button>
                        <Button
                          size="small"
                          icon={<DownloadOutlined />}
                          onClick={() => handleExport(v)}
                          loading={restoring === v.key}
                        >
                          导出
                        </Button>
                        <Popconfirm
                          title="删除此版本？"
                          description="此操作不可逆"
                          onConfirm={() => handleDelete(v)}
                        >
                          <Button
                            size="small"
                            danger
                            icon={<DeleteOutlined />}
                            loading={restoring === v.key}
                          >
                            删除
                          </Button>
                        </Popconfirm>
                      </Space>
                    ),
                  },
                ]}
              />
            ),
          }}
          columns={[
            { title: "文件路径", dataIndex: "rel" },
            { title: "版本数", dataIndex: "count", width: 80 },
            {
              title: "最近一次",
              dataIndex: "newest",
              width: 180,
              render: (n: VersionInfo) =>
                new Date(n.timestampMs).toLocaleString(),
            },
          ]}
        />
      )}
    </>
  );
}
