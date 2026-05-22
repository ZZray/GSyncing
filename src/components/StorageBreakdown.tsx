import { useEffect, useState } from "react";
import { Card, Empty, Spin, Tooltip } from "antd";
import { useAppStore } from "@/store";
import { computeSaveSize } from "@/api";
import type { GameProfile } from "@/types";

/**
 * Stacked horizontal bar showing how each game's local save footprint
 * contributes to the total. Single-segment per game, color from a
 * deterministic palette so it doesn't shift across renders.
 */

const PALETTE = [
  "#5b8def",
  "#8a6cff",
  "#52c41a",
  "#fa8c16",
  "#ff6b6b",
  "#a55eea",
  "#26de81",
  "#fdcb6e",
  "#778ca3",
  "#eb3b5a",
];

function humanBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

interface Row {
  game: GameProfile;
  bytes: number;
  color: string;
}

export default function StorageBreakdown() {
  const games = useAppStore((s) => s.games);
  const [rows, setRows] = useState<Row[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    Promise.all(
      games.map((g) =>
        computeSaveSize(g.id)
          .then((s) => ({ id: g.id, bytes: s.totalBytes }))
          .catch(() => ({ id: g.id, bytes: 0 }))
      )
    ).then((sizes) => {
      if (cancelled) return;
      const map = Object.fromEntries(sizes.map((s) => [s.id, s.bytes]));
      const data = games
        .map((g, i) => ({
          game: g,
          bytes: map[g.id] ?? 0,
          color: PALETTE[i % PALETTE.length],
        }))
        .filter((r) => r.bytes > 0)
        .sort((a, b) => b.bytes - a.bytes);
      setRows(data);
    });
    return () => {
      cancelled = true;
    };
  }, [games]);

  if (rows === null) {
    return (
      <Card title="存储用量" size="small" style={{ marginBottom: 18 }}>
        <div style={{ textAlign: "center", padding: 20 }}>
          <Spin />
        </div>
      </Card>
    );
  }

  if (rows.length === 0) {
    return (
      <Card title="存储用量" size="small" style={{ marginBottom: 18 }}>
        <Empty description="还没有可统计的本地存档" />
      </Card>
    );
  }

  const total = rows.reduce((s, r) => s + r.bytes, 0);

  return (
    <Card
      title={
        <span>
          存储用量 <span style={{ color: "#888", fontSize: 12 }}>· 本地存档共 {humanBytes(total)}</span>
        </span>
      }
      size="small"
      style={{ marginBottom: 18 }}
    >
      <div
        style={{
          display: "flex",
          height: 22,
          borderRadius: 6,
          overflow: "hidden",
          background: "var(--bg-divider, #eef0f5)",
          marginBottom: 14,
        }}
      >
        {rows.map((r) => {
          const pct = (r.bytes / total) * 100;
          return (
            <Tooltip
              key={r.game.id}
              title={`${r.game.name} · ${humanBytes(r.bytes)} (${pct.toFixed(
                1
              )}%)`}
            >
              <div
                style={{
                  width: `${pct}%`,
                  background: r.color,
                  height: "100%",
                  cursor: "pointer",
                }}
              />
            </Tooltip>
          );
        })}
      </div>
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          gap: 12,
          fontSize: 12,
        }}
      >
        {rows.slice(0, 12).map((r) => (
          <div
            key={r.game.id}
            style={{ display: "flex", alignItems: "center", gap: 6 }}
          >
            <span
              style={{
                display: "inline-block",
                width: 10,
                height: 10,
                background: r.color,
                borderRadius: 2,
              }}
            />
            <span>{r.game.name}</span>
            <span style={{ color: "#888" }}>· {humanBytes(r.bytes)}</span>
          </div>
        ))}
        {rows.length > 12 && (
          <span style={{ color: "#999" }}>+ 其它 {rows.length - 12} 个</span>
        )}
      </div>
    </Card>
  );
}
