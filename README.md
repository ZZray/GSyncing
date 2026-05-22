# GSyncing

[![CI](https://github.com/ZZray/GSyncing/actions/workflows/ci.yml/badge.svg)](https://github.com/ZZray/GSyncing/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/ZZray/GSyncing?display_name=tag&sort=semver)](https://github.com/ZZray/GSyncing/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Tauri 2](https://img.shields.io/badge/Tauri-2.x-24C8DB?logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-stable-orange?logo=rust&logoColor=white)](https://www.rust-lang.org/)

> 游戏存档云同步工具。Tauri 2 + Rust + React + Antd。

把游戏存档安全地同步到你自己的 S3 兼容存储（腾讯云 COS / 阿里云 OSS / AWS S3 / MinIO）或 WebDAV（坚果云 / Nextcloud）。  
专为多结局存档对比、跨机器开档、Roguelike 关键节点保存而设计。

## 截图

<table>
<tr>
<td><img src="docs/screenshots/library.png" alt="游戏库" width="420"/></td>
<td><img src="docs/screenshots/sync-status.png" alt="同步状态 + 存储用量" width="420"/></td>
</tr>
<tr>
<td><img src="docs/screenshots/versions.png" alt="历史版本 + 命名快照" width="420"/></td>
<td><img src="docs/screenshots/dark.png" alt="深色模式" width="420"/></td>
</tr>
</table>

> 截图待补 — `docs/screenshots/` 目录下放对应文件即可。

## 下载

最新 release：[**GitHub Releases**](https://github.com/ZZray/GSyncing/releases/latest)

- `GSyncing_*.exe` — NSIS 安装器（推荐）
- `GSyncing_*.msi` — 企业部署
- 内置自更新：装好后下一版自动弹窗，不用每次手动下载

## 特性矩阵

| 功能 | v1.0 状态 | 借鉴自 |
|---|---|---|
| **多云后端** | ✅ S3 (COS/OSS/AWS) + WebDAV | rclone |
| **双向同步** | ✅ SHA-256 内容指纹 + prior-snapshot 删除检测 | FreeFileSync |
| **mtime+size 快速比对** | ✅ 未变文件跳过重哈希（10-100× 提速） | FreeFileSync |
| **并发 IO + 重试** | ✅ buffer_unordered(N) + 指数退避 | rclone |
| **S3 大文件流式** | ✅ >64 MiB 用 `ByteStream::from_path`，内存不爆 | AWS SDK |
| **版本保留**（auto） | ✅ 远端 `.gsyncing/versions/`，默认保留 5 个 | Syncthing |
| **命名快照**（manual） | ✅ 永不自动清理，适合多结局存档对比 | 自有 |
| **冲突 rename-both** | ✅ 默认策略，不丢数据 | FreeFileSync |
| **Dry-run 预览** | ✅ 同步前显示将要做的事 | FreeFileSync |
| **实时进度 + ETA** | ✅ 5s 滑动窗口速率 + 剩余时间 | 自有 |
| **取消正在进行的同步** | ✅ CancellationToken + 取消按钮 | rclone |
| **自动同步触发** | ✅ 文件监控 + 周期 + 进程退出（Windows） | 自有 |
| **系统托盘** | ✅ 关闭窗口后台运行 + 托盘菜单 | 自有 |
| **凭据加密** | ✅ Windows DPAPI；非 Windows 待 Keychain | Chromium |
| **配置导入 / 导出** | ✅ 含 / 不含凭据可选 | 自有 |
| **per-game 后端** | ✅ 不同游戏路由到不同云 | 自有 |
| **首次启动引导** | ✅ 3 步 wizard | 自有 |
| **WebDAV 原生 COPY** | ✅ v1.1 raw HTTP COPY，COPY 不支持时自动 fallback get+put | RFC 4918 |
| **全局带宽限制** | ✅ v1.1 token bucket，settings 调 `maxBytesPerSec` | rclone --bwlimit |
| **卡片实时速度徽章** | ✅ v1.1 同步中游戏卡片右上角显示百分比 + phase 箭头 | 自有 |
| **远端锁**（多机互斥） | ✅ v1.2 `.gsyncing/lock.json` + TTL 5min + heartbeat | 自有 |
| **同步统计仪表板** | ✅ v1.2 累计次数 / 成功率 / 累计字节 / 最近 50 次记录 | 自有 |
| **stats 自动滚动** | ✅ v1.3 文件 >5000 条自动裁剪 | 自有 |
| **系统通知** | ✅ v1.3 同步完成/失败桌面 toast | tauri-plugin-notification |

## 快速上手

依赖：Node 20+ / Rust 1.77+ / Windows 10+（WebView2 自带）。

```pwsh
# 开发模式
npm install
npm run tauri:dev

# 发布构建（出 .msi 安装包）
npm run tauri:build
# 产物在 src-tauri/target/release/bundle/msi/
```

首次启动会弹引导 wizard，按提示填后端凭据 + 选一个内置游戏预设即可开始用。

## 同步语义

### 三种方向

- **Auto（双向）**：默认。比对本地、远端、上次快照三方状态，决定每个文件 add / modify / delete_local / delete_remote。watcher 触发的自动同步强制走 **Push** 避免覆盖玩家正在写的存档。
- **Push（推送）**：本地为准，远端 mirror。
- **Pull（拉取）**：远端为准，本地 mirror（只删被 prior 知道的）。

### 冲突解决

按 settings 里的策略：

| 策略 | 行为 |
|---|---|
| **rename-both**（默认） | 两份都保留，败方加 `.local-<ts>` / `.remote-<ts>` 后缀 |
| newer-wins | 修改时间晚的胜 |
| local-wins / remote-wins | 字面意思 |

只在双方都改过的真冲突场景才走策略；单边改则直接覆盖。

### 版本管理

```
games/<game-id>/
├── <实际文件>                       # 当前版本
└── .gsyncing/
    ├── index.json                  # 远端清单
    ├── versions/<rel>.<unix-ms>    # 自动版本（按 N 滚动）
    └── snapshots/
        ├── manifests/<id>.json     # 命名快照元数据
        └── files/<id>/<rel>        # 命名快照内容
```

**自动版本**：每次覆盖前归档，按 settings 的 N 保留最近 N 个，超出滚动删除。  
**命名快照**：用户手动打标签（如「决战前」），永不被自动清理，必须显式删除。

## 架构

```
                ┌─────────────────────────────────┐
                │   React 18 + Antd 5 (TS)        │
                │  GameLibrary  Settings  Logs    │
                │     ProgressBar  VersionDrawer  │
                └────────────┬────────────────────┘
                             │ tauri::invoke
                ┌────────────▼────────────────────┐
                │       commands/ (Rust)          │
                │   list_games / sync_one ...     │
                └────────────┬────────────────────┘
                             │
       ┌──────────────┬──────┴──────┬────────────┐
       ▼              ▼             ▼            ▼
   AppState       sync/engine    storage/      tray
   (RwLock)       (plan/exec)    (S3+WebDAV)
       │              │             │
       │              ▼             ▼
       │         scanner.rs    RetryingBackend
       │         (mtime+size      (指数退避)
       │          hash 缓存)
       │              │
       ▼              ▼
   watcher       snapshot.rs    crypto.rs
   process_watch (命名快照)     (DPAPI)
```

## 数据位置

- 配置 + 加密凭据：`%LOCALAPPDATA%/GSyncing/config.json`
- 本地 prior snapshot：`%LOCALAPPDATA%/GSyncing/snapshots/<game-id>.json`
- 日志（ring buffer + 落盘）：`%LOCALAPPDATA%/GSyncing/gsyncing.log`

## 安全说明

- **Windows**：凭据用 DPAPI 加密落盘，绑定当前 Windows 用户。攻击者拷走 config.json 无法在其它账户或机器解密。重装 Windows 后 DPAPI master key 销毁，旧密文会失效，需要重新填密钥。
- **非 Windows**：当前 base64 编码不加密。后续会接 macOS Keychain / Linux Secret Service。
- 程序不主动上传任何遥测数据，不与任何 GSyncing 自有服务通信。所有数据只去你自己配置的云后端。

## 性能 / 内存边界

- 单文件 ≤ 64 MiB 走 buffered 路径，> 64 MiB 走流式。
- 峰值内存上限 ≈ `max_concurrency × 单文件大小`（buffered 模式下）。
- mtime+size 复用 SHA：FreeFileSync 同款取舍。若文件被改但 size+mtime 偶然一致（极罕见），会漏判。

## 测试

```pwsh
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

24 个单测覆盖：路径展开 / 大小写无关替换 / SHA 缓存复用 / glob 过滤 / AppError 序列化 / 冲突决议矩阵 / push & pull plan / DPAPI round-trip / base64。

## 贡献

- 代码规范：`cargo fmt` + ESLint（前端默认配置）
- 一键自检：**`npm run release-check`**（build + smoke test + 36 Rust 单测，必须全绿才能发版）
- 中文回复 / 中文注释 / 英文 log message（grep 友好）
- 关键路径加 INFO 级日志（跨进程调用 / 长动作 / 重试 / 异常 catch）
- 看 [docs/TAURI2-GOTCHAS.md](docs/TAURI2-GOTCHAS.md) 避免 Tauri 2 + Vite + WebView2 的已知坑（manualChunks TDZ、crossorigin、base 路径等）

## License

MIT — 见 [LICENSE](LICENSE)

## 致谢

- [Ludusavi](https://github.com/mtkennerly/ludusavi) — 游戏档 manifest 概念
- [FreeFileSync](https://freefilesync.org/) — 同步语义、prior-snapshot 删除检测、rename-on-conflict、dry-run 预览
- [rclone](https://rclone.org/) — 重试 / 并发 / bisync 思路
- [Syncthing](https://syncthing.net/) — 版本保留设计
- [Tauri](https://tauri.app/) / [Antd](https://ant.design/) — 桌面应用框架
