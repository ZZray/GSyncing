import { useEffect, useMemo, useRef, useState } from "react";
import {
  Row,
  Col,
  Button,
  Empty,
  Dropdown,
  App as AntdApp,
  Modal,
  Input,
  Select,
  Space,
  Checkbox,
} from "antd";
import {
  PlusOutlined,
  CloudUploadOutlined,
  CloudDownloadOutlined,
  SyncOutlined,
  DeleteOutlined,
  EditOutlined,
  MoreOutlined,
  ExclamationCircleOutlined,
  HistoryOutlined,
  PushpinOutlined,
  PushpinFilled,
} from "@ant-design/icons";
import { createSnapshot, computeSaveSize } from "@/api";
import type { GameSizeInfo } from "@/types";
import { errMsg } from "@/api/err";
import { useAppStore } from "@/store";
import type { GameProfile, SyncState } from "@/types";
import GameEditor from "@/components/GameEditor";
import SyncPreviewModal from "@/components/SyncPreviewModal";
import VersionDrawer from "@/components/VersionDrawer";

const stateLabel: Record<SyncState, string> = {
  idle: "未同步",
  scanning: "扫描中",
  uploading: "上传中",
  downloading: "下载中",
  synced: "已同步",
  dirty: "有改动",
  error: "错误",
};

const stateClass: Record<SyncState, string> = {
  idle: "idle",
  scanning: "syncing",
  uploading: "syncing",
  downloading: "syncing",
  synced: "synced",
  dirty: "dirty",
  error: "error",
};

function humanBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

/** Per-category gradient applied to the game card cover. Falls back to the
 *  app's default blue-purple when the game has no category set. */
const CATEGORY_GRADIENT: Record<string, string> = {
  RPG: "linear-gradient(135deg, #5b8def 0%, #8a6cff 100%)",
  Action: "linear-gradient(135deg, #ff6b6b 0%, #ff9f43 100%)",
  Strategy: "linear-gradient(135deg, #20bf6b 0%, #26de81 100%)",
  Roguelike: "linear-gradient(135deg, #a55eea 0%, #e056fd 100%)",
  Sandbox: "linear-gradient(135deg, #fdcb6e 0%, #76b852 100%)",
  Other: "linear-gradient(135deg, #778ca3 0%, #a5b1c2 100%)",
};

const CATEGORY_LABELS: Record<string, string> = {
  RPG: "RPG",
  Action: "动作",
  Strategy: "策略",
  Roguelike: "Rogue",
  Sandbox: "沙盒",
  Other: "其它",
};

export default function GameLibrary() {
  const games = useAppStore((s) => s.games);
  const statusMap = useAppStore((s) => s.statusMap);
  const syncOne = useAppStore((s) => s.syncOne);
  const deleteGame = useAppStore((s) => s.deleteGame);
  const alwaysPreview = useAppStore((s) => s.settings?.alwaysPreview ?? true);
  const progress = useAppStore((s) => s.progress);
  const { message } = AntdApp.useApp();

  const [editorOpen, setEditorOpen] = useState(false);
  const [editing, setEditing] = useState<GameProfile | null>(null);
  const [previewState, setPreviewState] = useState<{
    game: GameProfile;
    direction: "auto" | "push" | "pull";
  } | null>(null);
  const [versionsFor, setVersionsFor] = useState<GameProfile | null>(null);
  const [query, setQuery] = useState("");
  const [sortKey, setSortKey] = useState<"added" | "name" | "recent">("added");
  const [sizes, setSizes] = useState<Record<string, GameSizeInfo>>({});
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const sizeCacheRef = useRef<Record<string, number>>({});

  // Lazy load per-game size with 30s TTL caching. Failed scans don't retry
  // for the same 30s window — keeps the UI responsive even when a game has
  // a misconfigured path.
  useEffect(() => {
    const now = Date.now();
    for (const g of games) {
      const last = sizeCacheRef.current[g.id] ?? 0;
      if (now - last > 30_000) {
        sizeCacheRef.current[g.id] = now;
        computeSaveSize(g.id)
          .then((info) =>
            setSizes((prev) => ({ ...prev, [g.id]: info }))
          )
          .catch(() => {
            /* leave previous value if any */
          });
      }
    }
  }, [games]);

  const filteredSorted = useMemo(() => {
    const q = query.trim().toLowerCase();
    let list = q
      ? games.filter((g) => g.name.toLowerCase().includes(q))
      : games.slice();
    switch (sortKey) {
      case "name":
        list.sort((a, b) => a.name.localeCompare(b.name, "zh"));
        break;
      case "recent": {
        list.sort((a, b) => {
          const ta = statusMap[a.id]?.lastSyncAt ?? 0;
          const tb = statusMap[b.id]?.lastSyncAt ?? 0;
          return tb - ta;
        });
        break;
      }
      // "added" — keep original insertion order
    }
    // Pinned games always sort above non-pinned, regardless of the sort
    // key. Within the pinned partition the chosen sort still applies.
    list.sort((a, b) => Number(!!b.pinned) - Number(!!a.pinned));
    return list;
  }, [games, statusMap, query, sortKey]);

  const togglePin = async (g: GameProfile, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await useAppStore
        .getState()
        .saveGame({ ...g, pinned: !g.pinned });
    } catch (err) {
      message.error(`置顶失败：${errMsg(err)}`);
    }
  };

  const toggleSelect = (gid: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(gid)) next.delete(gid);
      else next.add(gid);
      return next;
    });
  };

  const bulkSync = async (direction: "auto" | "push" | "pull") => {
    const ids = [...selected];
    if (ids.length === 0) return;
    message.info(`开始批量同步 ${ids.length} 个游戏 (${direction})`);
    // Engine has per-game lock — concurrent calls will serialize naturally.
    // Fire them in parallel; engine handles ordering.
    for (const id of ids) {
      const g = games.find((x) => x.id === id);
      if (!g) continue;
      try {
        await syncOne(id, direction);
      } catch (e) {
        message.error(`${g.name}: ${errMsg(e)}`);
      }
    }
    setSelected(new Set());
  };

  const openCreate = () => {
    setEditing(null);
    setEditorOpen(true);
  };
  const openEdit = (g: GameProfile) => {
    setEditing(g);
    setEditorOpen(true);
  };

  const handleSync = async (
    g: GameProfile,
    direction: "auto" | "push" | "pull"
  ) => {
    if (alwaysPreview) {
      setPreviewState({ game: g, direction });
      return;
    }
    try {
      await syncOne(g.id, direction);
      message.success(`已触发 ${g.name} 的同步`);
    } catch (e: unknown) {
      message.error(`同步失败：${errMsg(e)}`);
    }
  };

  const confirmDelete = (g: GameProfile) => {
    Modal.confirm({
      title: `确认删除 ${g.name}？`,
      icon: <ExclamationCircleOutlined />,
      content: "存档本身不会被删除，只是从同步列表中移除。",
      okType: "danger",
      okText: "删除",
      cancelText: "取消",
      onOk: () => deleteGame(g.id),
    });
  };

  const promptSnapshot = (g: GameProfile) => {
    let typed = "";
    Modal.confirm({
      title: `为 ${g.name} 创建命名快照`,
      icon: <PushpinOutlined style={{ color: "#8a6cff" }} />,
      content: (
        <div>
          <div style={{ color: "#666", marginBottom: 8 }}>
            命名快照永远不会被自动清理。适合标记关键决策点，方便后续切回某条路线。
          </div>
          <Input
            autoFocus
            placeholder="例如：黑暗剧情线 - 决战前"
            maxLength={100}
            onChange={(e) => {
              typed = e.target.value;
            }}
          />
        </div>
      ),
      okText: "创建",
      cancelText: "取消",
      onOk: async () => {
        const name = typed.trim();
        if (!name) {
          message.error("请输入快照名称");
          return Promise.reject();
        }
        try {
          const s = await createSnapshot(g.id, name);
          message.success(`已创建快照「${s.name}」（${s.fileCount} 文件）`);
        } catch (e: unknown) {
          message.error(`创建失败：${errMsg(e)}`);
          throw e;
        }
      },
    });
  };

  return (
    <>
      <div className="page-header">
        <div>
          <h2 className="page-title">游戏库</h2>
          <div className="page-subtitle">
            {games.length} 个游戏 · 点击卡片查看详情，或直接触发同步
          </div>
        </div>
        <Space>
          {selected.size > 0 && (
            <Space.Compact>
              <Button
                type="primary"
                icon={<SyncOutlined />}
                onClick={() => bulkSync("auto")}
              >
                批量同步 ({selected.size})
              </Button>
              <Dropdown
                menu={{
                  items: [
                    {
                      key: "push",
                      icon: <CloudUploadOutlined />,
                      label: "全部推送（覆盖远端）",
                      onClick: () => bulkSync("push"),
                    },
                    {
                      key: "pull",
                      icon: <CloudDownloadOutlined />,
                      label: "全部拉取（覆盖本地）",
                      onClick: () => bulkSync("pull"),
                    },
                    { type: "divider" },
                    {
                      key: "clear",
                      label: "清除选择",
                      onClick: () => setSelected(new Set()),
                    },
                  ],
                }}
              >
                <Button type="primary" icon={<MoreOutlined />} />
              </Dropdown>
            </Space.Compact>
          )}
          <Input.Search
            placeholder="搜索游戏..."
            allowClear
            onSearch={setQuery}
            onChange={(e) => {
              if (!e.target.value) setQuery("");
            }}
            style={{ width: 200 }}
          />
          <Select
            value={sortKey}
            onChange={setSortKey}
            style={{ width: 140 }}
            options={[
              { label: "按添加顺序", value: "added" },
              { label: "按名字 A-Z", value: "name" },
              { label: "最近同步", value: "recent" },
            ]}
          />
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
            添加游戏
          </Button>
        </Space>
      </div>

      {games.length === 0 ? (
        <div className="empty-library">
          <div className="empty-library-logo">G</div>
          <h3>还没有添加游戏</h3>
          <p>
            添加游戏后，存档会自动同步到你的云存储，
            <br />
            支持多机切换、版本回滚、命名快照（多结局存档）等。
          </p>
          <Button
            type="primary"
            size="large"
            icon={<PlusOutlined />}
            onClick={openCreate}
            style={{ marginTop: 12 }}
          >
            添加第一个游戏
          </Button>
          <div className="empty-library-hint">
            或从预设快速开始：原神 · 巫师 3 · Elden Ring · Stardew Valley ...
            共 23 款
          </div>
        </div>
      ) : filteredSorted.length === 0 ? (
        <Empty description={`没有匹配「${query}」的游戏`} />
      ) : (
        <Row gutter={[16, 16]}>
          {filteredSorted.map((g) => {
            const status = statusMap[g.id];
            const state: SyncState = status?.state ?? "idle";
            const live = progress && progress.gameId === g.id ? progress : null;
            const size = sizes[g.id];
            return (
              <Col key={g.id} xs={24} sm={12} md={8} lg={6} xxl={4}>
                <div className="game-card" onClick={() => openEdit(g)}>
                  <div
                    className="game-card-cover"
                    style={
                      g.category && CATEGORY_GRADIENT[g.category]
                        ? { background: CATEGORY_GRADIENT[g.category] }
                        : undefined
                    }
                  >
                    <div
                      className={`game-card-select ${
                        selected.has(g.id) ? "selected" : ""
                      }`}
                      onClick={(e) => toggleSelect(g.id, e)}
                    >
                      <Checkbox
                        checked={selected.has(g.id)}
                        onClick={(e) => e.stopPropagation()}
                        onChange={() => {
                          /* handled by parent onClick */
                        }}
                      />
                    </div>
                    {g.cover ?? g.name.slice(0, 2).toUpperCase()}
                    {g.category && CATEGORY_LABELS[g.category] && (
                      <div
                        style={{
                          position: "absolute",
                          bottom: 6,
                          left: 8,
                          background: "rgba(0, 0, 0, 0.35)",
                          color: "#fff",
                          fontSize: 10,
                          fontWeight: 500,
                          padding: "2px 6px",
                          borderRadius: 4,
                          letterSpacing: 0.3,
                          backdropFilter: "blur(4px)",
                        }}
                      >
                        {CATEGORY_LABELS[g.category]}
                      </div>
                    )}
                    {live && live.total > 0 && (
                      <div
                        style={{
                          position: "absolute",
                          top: 8,
                          right: 8,
                          background: "rgba(0,0,0,0.55)",
                          color: "#fff",
                          padding: "2px 8px",
                          borderRadius: 6,
                          fontSize: 11,
                          fontWeight: 500,
                          letterSpacing: 0.2,
                          backdropFilter: "blur(4px)",
                        }}
                      >
                        {live.phase === "upload" && "↑ "}
                        {live.phase === "download" && "↓ "}
                        {live.phase === "snapshot" && "📌 "}
                        {live.phase === "restore-snapshot" && "⏮ "}
                        {Math.round(
                          (live.bytesDone / Math.max(live.bytesTotal, 1)) * 100
                        )}
                        %
                      </div>
                    )}
                    <div
                      onClick={(e) => togglePin(g, e)}
                      title={g.pinned ? "取消置顶" : "置顶到顶部"}
                      style={{
                        position: "absolute",
                        top: 6,
                        right: live && live.total > 0 ? 56 : 8,
                        width: 26,
                        height: 26,
                        borderRadius: 6,
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "center",
                        color: g.pinned
                          ? "#ffd66e"
                          : "rgba(255, 255, 255, 0.55)",
                        cursor: "pointer",
                        transition: "all 0.15s",
                        background: g.pinned
                          ? "rgba(0, 0, 0, 0.35)"
                          : "transparent",
                      }}
                      onMouseEnter={(e) =>
                        ((e.currentTarget as HTMLDivElement).style.background =
                          "rgba(0, 0, 0, 0.35)")
                      }
                      onMouseLeave={(e) =>
                        ((e.currentTarget as HTMLDivElement).style.background =
                          g.pinned ? "rgba(0, 0, 0, 0.35)" : "transparent")
                      }
                    >
                      {g.pinned ? (
                        <PushpinFilled style={{ fontSize: 14 }} />
                      ) : (
                        <PushpinOutlined style={{ fontSize: 14 }} />
                      )}
                    </div>
                  </div>
                  <div className="game-card-body">
                    <div className="game-card-title">{g.name}</div>
                    <div className="game-card-meta">
                      <span className={`status-dot ${stateClass[state]}`} />
                      {stateLabel[state]}
                      {status?.lastSyncAt
                        ? ` · ${new Date(
                            status.lastSyncAt
                          ).toLocaleTimeString()}`
                        : ""}
                    </div>
                    {size && (
                      <div
                        style={{
                          color: "#999",
                          fontSize: 11,
                          marginBottom: 6,
                          marginTop: -4,
                        }}
                      >
                        {humanBytes(size.totalBytes)} · {size.fileCount} 文件
                      </div>
                    )}
                    <div onClick={(e) => e.stopPropagation()}>
                      <Button.Group size="small">
                        <Button
                          icon={<SyncOutlined />}
                          onClick={() => handleSync(g, "auto")}
                        >
                          同步
                        </Button>
                        <Dropdown
                          menu={{
                            items: [
                              {
                                key: "push",
                                icon: <CloudUploadOutlined />,
                                label: "上传 (覆盖远端)",
                                onClick: () => handleSync(g, "push"),
                              },
                              {
                                key: "pull",
                                icon: <CloudDownloadOutlined />,
                                label: "下载 (覆盖本地)",
                                onClick: () => handleSync(g, "pull"),
                              },
                              {
                                key: "snapshot",
                                icon: <PushpinOutlined />,
                                label: "标记当前为快照",
                                onClick: () => promptSnapshot(g),
                              },
                              {
                                key: "history",
                                icon: <HistoryOutlined />,
                                label: "查看历史版本",
                                onClick: () => setVersionsFor(g),
                              },
                              {
                                key: "edit",
                                icon: <EditOutlined />,
                                label: "编辑",
                                onClick: () => openEdit(g),
                              },
                              { type: "divider" },
                              {
                                key: "delete",
                                icon: <DeleteOutlined />,
                                danger: true,
                                label: "删除",
                                onClick: () => confirmDelete(g),
                              },
                            ],
                          }}
                        >
                          <Button icon={<MoreOutlined />} />
                        </Dropdown>
                      </Button.Group>
                    </div>
                  </div>
                </div>
              </Col>
            );
          })}
        </Row>
      )}

      <GameEditor
        open={editorOpen}
        initial={editing}
        onClose={() => setEditorOpen(false)}
      />

      {previewState && (
        <SyncPreviewModal
          open={!!previewState}
          gameId={previewState.game.id}
          gameName={previewState.game.name}
          direction={previewState.direction}
          onClose={() => setPreviewState(null)}
        />
      )}

      {versionsFor && (
        <VersionDrawer
          open={!!versionsFor}
          game={versionsFor}
          onClose={() => setVersionsFor(null)}
        />
      )}
    </>
  );
}
