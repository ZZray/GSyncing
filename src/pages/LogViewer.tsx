import { useEffect, useMemo, useRef, useState } from "react";
import { Select, Button, Input, Space, App as AntdApp } from "antd";
import { FolderOpenOutlined, SaveOutlined } from "@ant-design/icons";
import { openPath } from "@tauri-apps/plugin-opener";
import { useAppStore } from "@/store";
import { flushLog, getDataDir } from "@/api";
import { errMsg } from "@/api/err";
import type { LogEntry } from "@/types";

const LEVELS: Array<LogEntry["level"]> = ["info", "warn", "error", "debug"];

export default function LogViewer() {
  const logs = useAppStore((s) => s.logs);
  const [levels, setLevels] = useState<Array<LogEntry["level"]>>([
    "info",
    "warn",
    "error",
  ]);
  const [query, setQuery] = useState("");
  const [follow, setFollow] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);
  const { message } = AntdApp.useApp();

  const handleOpenDataDir = async () => {
    try {
      const dir = await getDataDir();
      await openPath(dir);
    } catch (e: unknown) {
      message.error(`打开失败：${errMsg(e)}`);
    }
  };

  const handleExportLog = async () => {
    try {
      const path = await flushLog();
      message.success(`日志已写入 ${path}`);
    } catch (e: unknown) {
      message.error(`导出失败：${errMsg(e)}`);
    }
  };

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return logs.filter(
      (l) =>
        levels.includes(l.level) &&
        (q === "" ||
          l.message.toLowerCase().includes(q) ||
          l.scope.toLowerCase().includes(q))
    );
  }, [logs, levels, query]);

  useEffect(() => {
    if (follow && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [filtered, follow]);

  return (
    <>
      <div className="page-header">
        <div>
          <h2 className="page-title">日志</h2>
          <div className="page-subtitle">
            实时同步日志 · {filtered.length} / {logs.length} 条
          </div>
        </div>
        <Space>
          <Select
            mode="multiple"
            value={levels}
            onChange={setLevels}
            style={{ minWidth: 220 }}
            options={LEVELS.map((l) => ({ label: l, value: l }))}
            placeholder="日志级别"
          />
          <Input.Search
            placeholder="搜索关键字"
            allowClear
            onSearch={setQuery}
            style={{ width: 220 }}
          />
          <Button
            type={follow ? "primary" : "default"}
            onClick={() => setFollow(!follow)}
          >
            {follow ? "已跟随新日志" : "跟随新日志"}
          </Button>
          <Button icon={<SaveOutlined />} onClick={handleExportLog}>
            导出日志
          </Button>
          <Button icon={<FolderOpenOutlined />} onClick={handleOpenDataDir}>
            打开数据目录
          </Button>
        </Space>
      </div>
      <div
        ref={scrollRef}
        style={{
          background: "#fff",
          border: "1px solid #eef0f5",
          borderRadius: 10,
          maxHeight: "calc(100vh - 220px)",
          overflowY: "auto",
        }}
      >
        {filtered.length === 0 ? (
          <div className="empty-tip">暂无日志</div>
        ) : (
          filtered.map((l, i) => (
            <div
              key={`${l.ts}-${l.scope}-${i}`}
              className={`log-line ${l.level}`}
            >
              <span className="ts">
                {new Date(l.ts).toLocaleTimeString()}
              </span>
              <b>[{l.level.toUpperCase()}]</b>{" "}
              <span style={{ color: "#5b8def" }}>{l.scope}</span>:{" "}
              {l.message}
            </div>
          ))
        )}
      </div>
    </>
  );
}
