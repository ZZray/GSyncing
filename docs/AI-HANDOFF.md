# AI 接手指南

> 这页是给**下一个 AI**接手 GSyncing 项目时看的。比 [RELEASE.md](RELEASE.md) 更具体：
> 给出可直接复制粘贴的命令、确切文件路径、失败模式 → 修复对照表、红线规则。
>
> **第一次接手必读顺序**：
> 1. 本文档（你正在看）— 30 秒了解整体
> 2. [TAURI2-GOTCHAS.md](TAURI2-GOTCHAS.md) — 别再踩 4 轮白屏的坑
> 3. [RELEASE.md](RELEASE.md) — 给人看的、更详细的发版叙事
> 4. [CONTRIBUTING.md](../CONTRIBUTING.md) — 代码风格 + 关键路径日志规范

---

## 0. 30 秒项目快照

| 项 | 当前值 |
|---|---|
| 项目根 | `E:\GSyncing` |
| Repo | `https://github.com/ZZray/GSyncing.git` (main 分支) |
| 用户身份 | Ray Zhang `<1789958884@qq.com>` (commit 用) |
| 当前版本 | `2.1.0` (在 3 处 manifests 同步：`package.json` / `src-tauri/Cargo.toml` / `src-tauri/tauri.conf.json`) |
| 签名公钥 | embed 在 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey` |
| 签名私钥 | `C:\Users\loveu\.tauri\gsyncing.key`（minisign ed25519，无密码）**不入 git** |
| Updater endpoint | `https://github.com/ZZray/GSyncing/releases/latest/download/latest.json` |
| 测试基线 | 37 个 Rust 单测 + jsdom smoke test |
| CI | `.github/workflows/ci.yml`（每次 push 跑）+ `release.yml`（tag 触发自动发版）|

```
E:\GSyncing\
├── src/                      # React + Antd 前端（约 24 个组件/页面）
├── src-tauri/                # Rust 后端
│   ├── src/                  # 17 个模块
│   ├── icons/                # 6 尺寸 G logo ICO
│   ├── capabilities/         # Tauri 2 permissions
│   ├── tauri.conf.json       # ← 改版本时记得改
│   └── Cargo.toml            # ← 改版本时记得改
├── package.json              # ← 改版本时记得改
├── docs/
│   ├── AI-HANDOFF.md         # 你在看
│   ├── TAURI2-GOTCHAS.md     # 4 轮白屏教训
│   └── RELEASE.md            # 人看的发版叙事
├── scripts/smoke-test.mjs    # jsdom 抓 TDZ
└── .github/workflows/
    ├── ci.yml                # 每次 push 跑
    └── release.yml           # tag v* 触发
```

---

## 1. 一键自检（提任何 PR 前必跑）

```pwsh
# 在 E:\GSyncing 下：
npm run release-check
```

这条等价于：
```
tsc -b && vite build           # 前端编译
&& node scripts/smoke-test.mjs # jsdom 抓 TDZ / SyntaxError / vendor-chunk 红线
&& cargo test --manifest-path src-tauri/Cargo.toml --lib  # Rust 37 单测
```

**全绿才能 push。** 任意一步红就停下修，不要 push 红的代码。

如果你想分开跑：
```pwsh
npm run build         # 仅前端
npm run smoke         # 仅 smoke
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check  # 格式检查
```

---

## 2. 本地 dev 跑起来

```pwsh
cd E:\GSyncing
npm install            # 首次或依赖变化后
npm run tauri:dev      # 起开发模式，带热重载
```

跑起来后会有：
- 一个 Tauri 窗口（自动开 DevTools — Settings 里能关）
- 一个 vite dev server （http://localhost:1420）
- HMR：改 .tsx 即时刷，改 Rust 要重启

---

## 3. 本地手动签名 release build

如果你想本地跑一次签名 build（不通过 CI）：

```pwsh
# Powershell 必须 — Bash 的 export 语法在 Windows 走不通
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content "$HOME/.tauri/gsyncing.key" -Raw)
# 私钥没设密码，PASSWORD env 不需要

npm run tauri:build
```

**预期产物**（路径相对 `src-tauri/target/release/bundle/`）：

```
nsis/
├── GSyncing_2.1.0_x64-setup.exe          ~4.8 MB    NSIS 安装器
└── GSyncing_2.1.0_x64-setup.exe.sig      ~92 B      minisign 签名 ← 关键
msi/
├── GSyncing_2.1.0_x64_en-US.msi          ~7 MB      MSI 安装器
└── GSyncing_2.1.0_x64_en-US.msi.sig      ~92 B      minisign 签名
```

**没有 .sig 文件 = updater 启用没成功**。检查：
1. `src-tauri/tauri.conf.json` 的 `bundle.createUpdaterArtifacts` 是不是 `true`
2. `$env:TAURI_SIGNING_PRIVATE_KEY` 是否真的设了（`echo $env:TAURI_SIGNING_PRIVATE_KEY` 检查）

---

## 4. 发版（推荐：CI 自动化）

### 4.1 一次性配置 — 已经做完，不用重做

GitHub Repo → Settings → Secrets and variables → Actions → **New repository secret**：

| Name | Value 怎么拿 |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | `Get-Content "$HOME/.tauri/gsyncing.key" -Raw` → 全部内容粘贴 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 我们没设密码，**不要加这个 secret** 或留空 |

### 4.2 发版的标准 5 步（每次都这么走）

```pwsh
# 1. 决定下个版本号，比如 v2.2.0。同步 3 处版本字段：
#    - package.json: "version": "2.2.0"
#    - src-tauri/Cargo.toml: version = "2.2.0"
#    - src-tauri/tauri.conf.json: "version": "2.2.0"

# 2. 在 CHANGELOG.md 顶部加一节 "## v2.2.0 — <主题>"

# 3. 本地自检全绿
npm run release-check

# 4. 提交 + 推
git add -A
git commit -m "bump 版本到 2.2.0 + CHANGELOG"
git push

# 5. 打 tag 触发自动发版
git tag v2.2.0
git push origin v2.2.0
```

打 tag 后 ~10-15 分钟（Windows runner + Rust 首次编译慢），去
https://github.com/ZZray/GSyncing/releases 应该看到：

```
v2.2.0
├── GSyncing_2.2.0_x64-setup.exe       ← 用户下载
├── GSyncing_2.2.0_x64-setup.exe.sig   ← 签名
├── GSyncing_2.2.0_x64_en-US.msi
├── GSyncing_2.2.0_x64_en-US.msi.sig
└── latest.json                        ← 内置自更新读这个
```

**装在用户机器上的旧版本下次启动自动看到「发现新版本 v2.2.0」弹窗。**

### 4.3 手动备用流程（如果 CI 挂了）

```pwsh
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content "$HOME/.tauri/gsyncing.key" -Raw)
npm run tauri:build

# 手动生成 latest.json：
$version = "2.2.0"
$sig = Get-Content "src-tauri/target/release/bundle/nsis/GSyncing_${version}_x64-setup.exe.sig" -Raw
$manifest = @{
  version = $version
  notes = "v2.2.0 — <主题>"
  pub_date = (Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ")
  platforms = @{
    "windows-x86_64" = @{
      signature = $sig
      url = "https://github.com/ZZray/GSyncing/releases/download/v${version}/GSyncing_${version}_x64-setup.exe"
    }
  }
} | ConvertTo-Json -Depth 10
$manifest | Out-File latest.json -Encoding utf8 -NoNewline

# 上传到 GitHub Releases
gh release create "v$version" `
  --title "v$version" `
  --notes-file CHANGELOG.md `
  "src-tauri/target/release/bundle/nsis/GSyncing_${version}_x64-setup.exe" `
  "src-tauri/target/release/bundle/nsis/GSyncing_${version}_x64-setup.exe.sig" `
  "src-tauri/target/release/bundle/msi/GSyncing_${version}_x64_en-US.msi" `
  "src-tauri/target/release/bundle/msi/GSyncing_${version}_x64_en-US.msi.sig" `
  latest.json
```

---

## 5. 失败模式 → 修复对照表

### 5.1 编译类

| 现象 | 大概率原因 | 修复 |
|---|---|---|
| `cargo check` 报 trait/method 不存在 | aws-sdk-s3 / reqwest_dav / tauri 小版本升级 API 变动 | 翻该 crate CHANGELOG，对照修签名 |
| `tsc -b` 报 unused import | 你删了用法没删 import | 删掉 import |
| `tsc -b` 报路径解析失败 | `@/foo` alias 没在 tsconfig.json 配 | 看 `paths` 字段；vite.config.ts 的 `resolve.alias` 也要对上 |
| `vite build` 卡在 transforming | 90% 是依赖循环 | 看终端报哪个文件，断循环 |
| `cargo fmt --check` 失败 | 没跑 fmt | `cargo fmt --manifest-path src-tauri/Cargo.toml` |

### 5.2 运行时类（白屏 / 闪退）

| 症状 | 根因 | 修法 |
|---|---|---|
| 装好启动**白屏** + 标题栏图标显示 | 见 TAURI2-GOTCHAS.md。99% 是 vite/webview 协议层问题 | **第一步先点 F12 或看自动弹的 DevTools** |
| DevTools 红字 `Cannot access 'ms' before initialization` | manualChunks 把 `ms` / dayjs / debug 跨 chunk 切了 → ESM TDZ | `vite.config.ts` **不要**用 manualChunks，回单 bundle |
| Network 标签所有 .js 文件 404 | vite `base` 不对 | `vite.config.ts` 加 `base: "./"` |
| Console 安静但页面就是不动 | 默认 vite 给 script 加了 `crossorigin` attr → Tauri 2 不返 CORS header → 拒绝执行 | `vite.config.ts` 里 `stripCrossOriginPlugin` 必须存在并启用 |
| 装好启动 F12 也没反应 | release 没启 devtools feature | `src-tauri/Cargo.toml` 的 tauri features 加 `"devtools"`；setup hook 调 `win.open_devtools()` 条件门控在 settings 里 |

### 5.3 自更新类

| 症状 | 根因 | 修法 |
|---|---|---|
| 「检查更新」点了报 `InvalidSignature` | `latest.json` 的 signature 字段跟 .sig 文件不一致 | 重新从对应 `.sig` 文件 `Get-Content -Raw` 取内容，粘到 latest.json |
| 提示 `version <= current` 不更新 | latest.json 的 version 字段是 "v2.2.0"（带 v 前缀）或者小于当前 | 必须用 semver，例如 `"2.2.0"` 不是 `"v2.2.0"` |
| Endpoint 404 | release 还没上传 / latest.json 没挂上去 | 去 GitHub Releases 检查 assets 有没有 latest.json |
| 下载卡住不动 | release asset 是 private / 用户网络不通 GitHub | GitHub release 必须 public；客户端网络问题让用户自查 |
| 装好后启动闪退 / 旧版数据丢失 | 升级途中 config schema breaking change | 加 schema 兼容层 — 看 `state.rs::initialize` 的 unwrap_or_else 保护逻辑 |

### 5.4 CI 类

| 症状 | 修法 |
|---|---|
| `ci.yml` Windows job 失败 in cargo step | 看 log 是哪个测试挂；本地复现修了再 push |
| `ci.yml` asset-layout-guard 报红 | 你在某个 PR 里重新引入了 vendor-* / crossorigin / 绝对路径 — 别这样，去看 TAURI2-GOTCHAS.md |
| `release.yml` 失败在 "Tauri signed build" | `TAURI_SIGNING_PRIVATE_KEY` secret 没配 / 配错 | 重新去 repo secrets 配 |
| `release.yml` 失败在 "Generate latest.json" | 版本号没同步（找不到对应 .exe 文件名） | 检查 3 处 version 字段是否一致 + tag 名是否 v + 同样版本号 |

---

## 6. 红线（永远不要做的事）

排序按"踩了多痛"：

1. **不要在 vite.config.ts 加 `manualChunks`** — 触发 TDZ → 白屏，CI 也会拒
2. **不要在 vite.config.ts 删 `stripCrossOriginPlugin`** — WebView2 拒绝模块脚本 → 白屏
3. **不要把 vite `base` 改回 `/`** — 资源 404 → 白屏
4. **不要在 async fn 里直接调 `std::fs::*`** — 阻塞 tokio runtime，必须 `tokio::task::spawn_blocking`
5. **不要把 `~/.tauri/gsyncing.key` 提交进 git** — 私钥泄漏后所有签名失去意义
6. **不要换 `tauri.conf.json` 的 `plugins.updater.pubkey`** 除非同时换私钥并接受**所有老用户必须手动重装**
7. **不要 force push to main** — CLAUDE.md §7.3，必须先问
8. **不要把 commit 加 AI 署名** — CLAUDE.md §7.1，commit message 写改动本身即可

---

## 7. 关键命令速查卡

```pwsh
# 起开发
npm run tauri:dev

# 一键自检
npm run release-check

# 单独跑 cargo 单测
cargo test --manifest-path src-tauri/Cargo.toml --lib

# 单独跑 smoke
npm run smoke

# Rust 格式化
cargo fmt --manifest-path src-tauri/Cargo.toml

# 本地签名 release build
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content "$HOME/.tauri/gsyncing.key" -Raw); npm run tauri:build

# 看仓库状态
git status; git log --oneline -10

# 发版（CI 走完整 5 步流程见 §4.2）
git tag v2.x.x; git push origin v2.x.x
```

---

## 8. 项目里**不该**碰的文件

| 文件 | 为什么 |
|---|---|
| `node_modules/` | npm 自动管，volatile |
| `dist/` | vite build 产物 |
| `src-tauri/target/` | cargo build 产物 |
| `src-tauri/Cargo.lock` | gitignore 里有，每次自动重新生成 |
| `src-tauri/gen/schemas/` | tauri build 自动生成 |
| `~/.tauri/gsyncing.key` | **永远不要**入 git。私钥。 |
| `vite.config.d.ts` / `*.tsbuildinfo` | TS build cache，gitignore 里有 |
| `%LOCALAPPDATA%\GSyncing\*` | 运行时数据（config.json / snapshots / stats.jsonl），用户数据 |

---

## 9. 项目演进总览（v0.1 → v2.1.0）

完整 CHANGELOG 在 [../CHANGELOG.md](../CHANGELOG.md)。要点：

| 阶段 | 版本 | 主题 |
|---|---|---|
| 朴素 v1 | v0.1 | Tauri 2 + S3/WebDAV + 双向同步引擎 + UI |
| 性能 | v0.2 | mtime+size 预过滤 / 并发 / 重试 / 版本保留 |
| UX | v0.3 | dry-run 预览 / 版本回滚 / 进程退出触发 |
| 调试 | v0.4 | 进度事件 / 托盘 / S3 流式 |
| 控制 | v0.5 | 速度+ETA / Cancel / per-game 后端 |
| 多结局 | v0.6 | 命名快照（用户最强烈要求的功能）|
| 安全 | v0.7 | DPAPI 加密 / 配置 import/export / 版本单条管理 |
| 收敛冲刺 | v0.8 → v1.0 | wizard / bundle 拆分 / 单测 / LICENSE / 发版验证 |
| 性能复盘 | v1.1 | WebDAV 原生 COPY / 带宽限制 / 卡片速度徽章 |
| 多机 | v1.2 | 远端锁（heartbeat） / 统计仪表板 |
| 易用性 | v1.3 → v1.4 | 通知 / **4 轮白屏排查** / 最终单 bundle 修复 |
| 视觉 | v1.5 → v1.6 | 关于对话框 / 键盘快捷键 / 品类着色 / 34 预设 |
| 完整 | v1.7 | 深色模式 / 冲突 Ask 真交互 |
| 操作 | v1.8 → v1.9 | 批量同步 / 置顶 / 存储用量 / 路径验证 / 快照排序 |
| 工程 | v2.0 | GitHub Actions CI / asset-layout-guard / 托盘最近游戏 |
| **生产** | **v2.1** | **tauri-plugin-updater + 签名 + CI 自动发版** ← **当前** |

84 任务 / 37 Rust 单测 / 12 个版本迭代 / 4 轮白屏排查 / 1 个完整产品。

---

## 10. 你（AI）第一次接手时应该做的 3 件事

1. **不要 push 任何代码**直到先跑 `npm run release-check` 确认本地状态。
2. **看一眼 `git log --oneline -10`** 了解最近 commit 节奏。
3. **如果用户问"还能继续做什么"**，看 README 末尾的"未来候选"或 CHANGELOG 最新一节的"剩余真有用方向"。
