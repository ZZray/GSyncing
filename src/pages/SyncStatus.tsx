import { useEffect, useMemo, useState } from "react";
import { Table, Tag, Empty, Row, Col, Card, Statistic, Alert } from "antd";
import {
  CloudUploadOutlined,
  ClockCircleOutlined,
  CheckCircleOutlined,
  WarningOutlined,
} from "@ant-design/icons";
import type { ColumnsType } from "antd/es/table";
import { useAppStore } from "@/store";
import type { GameSyncStatus, SyncState, StatEntry } from "@/types";
import { readStats } from "@/api";
import { errMsg } from "@/api/err";
import StorageBreakdown from "@/components/StorageBreakdown";

const stateMeta: Record<SyncState, { color: string; label: string }> = {
  idle: { color: "default", label: "未同步" },
  scanning: { color: "processing", label: "扫描中" },
  uploading: { color: "processing", label: "上传中" },
  downloading: { color: "processing", label: "下载中" },
  synced: { color: "success", label: "已同步" },
  dirty: { color: "warning", label: "有改动" },
  error: { color: "error", label: "错误" },
};

interface Row extends GameSyncStatus {
  name: string;
}

const columns: ColumnsType<Row> = [
  { title: "游戏", dataIndex: "name", key: "name", width: 180 },
  {
    title: "状态",
    dataIndex: "state",
    key: "state",
    width: 110,
    render: (s: SyncState) => {
      const meta = stateMeta[s] ?? { color: "default", label: String(s) };
      return <Tag color={meta.color}>{meta.label}</Tag>;
    },
  },
  { title: "本地文件数", dataIndex: "localFiles", key: "localFiles", width: 110 },
  { title: "远端文件数", dataIndex: "remoteFiles", key: "remoteFiles", width: 110 },
  {
    title: "最后同步",
    dataIndex: "lastSyncAt",
    key: "lastSyncAt",
    width: 180,
    render: (v?: number) => (v ? new Date(v).toLocaleString() : "—"),
  },
  {
    title: "消息",
    dataIndex: "message",
    key: "message",
    render: (msg?: string, row?: Row) => row?.lastError ?? msg ?? "",
  },
];

function humanBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function humanDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  return `${Math.floor(ms / 60000)}m${Math.round((ms % 60000) / 1000)}s`;
}

export default function SyncStatus() {
  const games = useAppStore((s) => s.games);
  const statusMap = useAppStore((s) => s.statusMap);
  const [stats, setStats] = useState<StatEntry[]>([]);
  const [statsErr, setStatsErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    readStats()
      .then(setStats)
      .catch((e) => setStatsErr(errMsg(e)))
      .finally(() => setLoading(false));
  }, [statusMap]); // refresh when any sync status changes

  const summary = useMemo(() => computeSummary(stats), [stats]);

  const data: Row[] = games.map((g) => {
    const status = statusMap[g.id] ?? {
      gameId: g.id,
      state: "idle" as SyncState,
      localFiles: 0,
      remoteFiles: 0,
    };
    return { ...status, name: g.name };
  });

  return (
    <>
      <div className="page-header">
        <div>
          <h2 className="page-title">同步状态</h2>
          <div className="page-subtitle">
            实时状态 · 累计统计 · 最近活动
          </div>
        </div>
      </div>

      {statsErr && <Alert type="warning" message={statsErr} showIcon style={{ marginBottom: 14 }} />}

      <Row gutter={[12, 12]} style={{ marginBottom: 18 }}>
        <Col xs={12} md={6}>
          <Card size="small">
            <Statistic
              title="累计同步次数"
              value={summary.total}
              prefix={<ClockCircleOutlined style={{ color: "#5b8def" }} />}
              loading={loading}
            />
          </Card>
        </Col>
        <Col xs={12} md={6}>
          <Card size="small">
            <Statistic
              title="成功"
              value={summary.success}
              prefix={<CheckCircleOutlined style={{ color: "#52c41a" }} />}
              loading={loading}
              suffix={summary.total > 0 ? `/ ${summary.total}` : ""}
            />
          </Card>
        </Col>
        <Col xs={12} md={6}>
          <Card size="small">
            <Statistic
              title="失败"
              value={summary.failures}
              prefix={<WarningOutlined style={{ color: "#ff4d4f" }} />}
              loading={loading}
            />
          </Card>
        </Col>
        <Col xs={12} md={6}>
          <Card size="small">
            <Statistic
              title="累计传输"
              value={humanBytes(summary.totalBytes)}
              prefix={<CloudUploadOutlined style={{ color: "#8a6cff" }} />}
              loading={loading}
            />
          </Card>
        </Col>
      </Row>

      <StorageBreakdown />

      <Card
        title="最近 50 次同步"
        size="small"
        style={{ marginBottom: 18 }}
        loading={loading}
      >
        {stats.length === 0 ? (
          <Empty description="还没有同步历史" />
        ) : (
          <Table
            size="small"
            rowKey={(r) => `${r.ts}-${r.gameId}`}
            pagination={{ pageSize: 10 }}
            dataSource={[...stats]
              .sort((a, b) => b.ts - a.ts)
              .slice(0, 50)
              .map((s) => ({
                ...s,
                gameName:
                  games.find((g) => g.id === s.gameId)?.name ?? s.gameId,
              }))}
            columns={[
              {
                title: "时间",
                dataIndex: "ts",
                width: 170,
                render: (n: number) => new Date(n).toLocaleString(),
              },
              { title: "游戏", dataIndex: "gameName", width: 140 },
              {
                title: "方向",
                dataIndex: "direction",
                width: 80,
                render: (d: string) => {
                  if (d.toLowerCase().includes("push"))
                    return <Tag color="blue">↑ 上传</Tag>;
                  if (d.toLowerCase().includes("pull"))
                    return <Tag color="green">↓ 下载</Tag>;
                  return <Tag>双向</Tag>;
                },
              },
              {
                title: "结果",
                dataIndex: "success",
                width: 80,
                render: (ok: boolean) =>
                  ok ? <Tag color="success">OK</Tag> : <Tag color="error">失败</Tag>,
              },
              {
                title: "耗时",
                dataIndex: "durationMs",
                width: 90,
                render: (n: number) => humanDuration(n),
              },
              {
                title: "错误",
                dataIndex: "error",
                ellipsis: true,
              },
            ]}
          />
        )}
      </Card>

      {data.length > 0 && (
        <Card title="实时状态" size="small">
          <Table
            rowKey="gameId"
            columns={columns}
            dataSource={data}
            pagination={false}
            bordered
            size="middle"
          />
        </Card>
      )}
    </>
  );
}

function computeSummary(stats: StatEntry[]) {
  let success = 0;
  let failures = 0;
  let totalBytes = 0;
  for (const s of stats) {
    if (s.success) success++;
    else failures++;
    totalBytes += s.totalBytes ?? 0;
  }
  return { total: stats.length, success, failures, totalBytes };
}
