import { useEffect, useRef, useState } from "react";
import { Progress, Tooltip, Button, App as AntdApp } from "antd";
import { CloseCircleOutlined } from "@ant-design/icons";
import { useAppStore } from "@/store";
import { cancelSync } from "@/api";
import { errMsg } from "@/api/err";

function humanBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function humanRate(bytesPerSec: number): string {
  if (!isFinite(bytesPerSec) || bytesPerSec <= 0) return "—";
  return `${humanBytes(bytesPerSec)}/s`;
}

function humanDuration(seconds: number): string {
  if (!isFinite(seconds) || seconds <= 0) return "—";
  if (seconds < 60) return `${Math.round(seconds)}s`;
  if (seconds < 3600)
    return `${Math.floor(seconds / 60)}m${Math.round(seconds % 60)}s`;
  return `${Math.floor(seconds / 3600)}h${Math.floor((seconds % 3600) / 60)}m`;
}

function phaseLabel(phase: string): { label: string; color: string; arrow: string } {
  switch (phase) {
    case "upload":
      return { label: "上传", color: "#5b8def", arrow: "↑" };
    case "download":
      return { label: "下载", color: "#52c41a", arrow: "↓" };
    case "snapshot":
      return { label: "创建快照", color: "#8a6cff", arrow: "📌" };
    case "restore-snapshot":
      return { label: "恢复快照", color: "#fa8c16", arrow: "⏮" };
    default:
      return { label: phase || "传输", color: "#5b8def", arrow: "·" };
  }
}

/** Sliding-window samples of (timestamp_ms, total_bytes_done). */
const WINDOW_MS = 5_000;

export default function ProgressBar() {
  const progress = useAppStore((s) => s.progress);
  const { message } = AntdApp.useApp();
  const samplesRef = useRef<Array<{ ts: number; bytes: number }>>([]);
  const lastGameRef = useRef<string | null>(null);
  const [, setTick] = useState(0);

  // Reset the rolling window whenever the active game changes.
  useEffect(() => {
    if (progress?.gameId !== lastGameRef.current) {
      samplesRef.current = [];
      lastGameRef.current = progress?.gameId ?? null;
    }
  }, [progress?.gameId]);

  // Push a sample whenever progress arrives. Filter to last WINDOW_MS.
  useEffect(() => {
    if (!progress) return;
    const now = Date.now();
    samplesRef.current.push({ ts: now, bytes: progress.bytesDone });
    samplesRef.current = samplesRef.current.filter(
      (s) => now - s.ts <= WINDOW_MS
    );
  }, [progress]);

  // Tick once per second so the speed/ETA labels keep updating between
  // backend events (e.g., during a large single-file upload).
  useEffect(() => {
    if (!progress) return;
    const id = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(id);
  }, [progress]);

  if (!progress || progress.total <= 0) return null;

  const samples = samplesRef.current;
  let bytesPerSec = 0;
  if (samples.length >= 2) {
    const first = samples[0];
    const last = samples[samples.length - 1];
    const dt = (last.ts - first.ts) / 1000;
    if (dt > 0) {
      bytesPerSec = Math.max(0, (last.bytes - first.bytes) / dt);
    }
  }
  // Fallback: average since startedAt if window is too short.
  if (bytesPerSec === 0 && progress.startedAt > 0) {
    const elapsed = (Date.now() - progress.startedAt) / 1000;
    if (elapsed > 0.5) {
      bytesPerSec = progress.bytesDone / elapsed;
    }
  }

  const remainingBytes = Math.max(progress.bytesTotal - progress.bytesDone, 0);
  const etaSec = bytesPerSec > 0 ? remainingBytes / bytesPerSec : Infinity;
  const percent = Math.round(
    (progress.bytesDone / Math.max(progress.bytesTotal, 1)) * 100
  );

  const handleCancel = async () => {
    try {
      const ok = await cancelSync(progress.gameId);
      if (ok) {
        message.info("已请求取消，等待当前文件完成");
      } else {
        message.warning("当前没有可取消的同步");
      }
    } catch (e: unknown) {
      message.error(`取消失败：${errMsg(e)}`);
    }
  };

  const phase = phaseLabel(progress.phase);
  return (
    <Tooltip
      placement="bottom"
      title={
        <>
          {phase.label} · {progress.current}/{progress.total} 文件
          <br />
          {humanBytes(progress.bytesDone)} /{" "}
          {humanBytes(progress.bytesTotal)} ({percent}%)
          <br />
          {humanRate(bytesPerSec)} · 剩余 {humanDuration(etaSec)}
          {progress.currentFile ? (
            <>
              <br />
              <span style={{ opacity: 0.8 }}>{progress.currentFile}</span>
            </>
          ) : null}
        </>
      }
    >
      <div
        style={{
          background: "#fff",
          padding: "6px 24px",
          borderBottom: "1px solid #eef0f5",
          display: "flex",
          alignItems: "center",
          gap: 12,
        }}
      >
        <div style={{ flex: 1 }}>
          <Progress
            percent={percent}
            size="small"
            strokeColor={phase.color}
            showInfo={false}
            style={{ marginBottom: 2 }}
          />
          <div
            style={{
              fontSize: 11,
              color: "#888",
              display: "flex",
              justifyContent: "space-between",
            }}
          >
            <span>
              {phase.arrow} {phase.label} · {humanRate(bytesPerSec)} · 剩{" "}
              {humanDuration(etaSec)}
            </span>
            <span>
              {progress.current}/{progress.total} ·{" "}
              {humanBytes(progress.bytesDone)}/
              {humanBytes(progress.bytesTotal)}
            </span>
          </div>
        </div>
        <Button
          size="small"
          danger
          type="text"
          icon={<CloseCircleOutlined />}
          onClick={handleCancel}
        >
          取消
        </Button>
      </div>
    </Tooltip>
  );
}
