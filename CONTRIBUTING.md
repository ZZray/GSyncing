# 贡献指南

谢谢有兴趣给 GSyncing 提 PR。这页讲清楚怎么本地开发、跑测试、提 PR 不被退回。

## 本地开发环境

| 工具 | 版本要求 |
|---|---|
| Node.js | 20 + |
| npm | 10 + |
| Rust | stable（rustup default stable）|
| Tauri 系统依赖 | Windows: 自带 WebView2；Linux/macOS: 参考 [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) |

## 起飞步骤

```bash
git clone https://github.com/ZZray/GSyncing.git
cd GSyncing
npm install
npm run tauri:dev   # 开发模式带热重载
```

第一次启动会自动弹「首次启动向导」。可以填一个测试用的 MinIO 或 S3 桶；不想真同步可以在 Settings 里把"启动时自动检查更新"关掉。

## 提交 PR 之前

**`npm run release-check` 必须四栈全绿**：

```bash
npm run release-check
```

这条命令一次性跑：
- `tsc -b` + `vite build` — 前端编译
- `node scripts/smoke-test.mjs` — jsdom 加载 dist bundle，专门抓 TDZ / SyntaxError / vendor-chunk / crossorigin 红线
- `cargo test --lib` — Rust 37 个单测

外加：

```bash
cd src-tauri
cargo fmt --all -- --check   # 格式化必须干净
```

PR 模板里有完整 checklist，照着勾。

## 代码风格

- **Rust**：rustfmt 默认配置。`cargo fmt --all` 前不要 push
- **TypeScript**：跟现有的来。strict mode，noUnusedLocals 都开着；导入语句 react 类放最前
- **CSS**：用 `var(--bg-app)` 这种变量，**不要硬编码颜色**，否则深色模式会破
- **注释 / log message**：注释用中文（中文 review 更顺），log message 用英文（grep 友好）
- **commit message**：中文 OK，描述"做了什么"+"为什么"，不要 AI 署名

## 关键路径必须加日志（INFO 级）

这条线踩过血的教训：

- 跨进程 / 跨服务调用（HTTP / IPC 双向）
- 状态机迁移（"前: X → 后: Y"）
- 长动作开始 + 结束（>1s 操作）
- 重试 / 兜底 / 异常 catch（带 err_type + message）
- 资源生命周期（连接 / 锁 / 订阅）
- 决策分支（"因 condition Y，走 X"）

debug 级用户默认看不到 — 关键路径不能放 debug。

## 已知坑

写代码前**强烈建议**扫一遍 [docs/TAURI2-GOTCHAS.md](docs/TAURI2-GOTCHAS.md)。
这文档总结了从 v1.3 到 v1.4.2 经历的 4 轮白屏排查教训：

- ❌ Vite `manualChunks` → ESM TDZ → 白屏
- ❌ Vite 默认 `crossorigin` 属性 → WebView2 拒绝模块脚本 → 白屏
- ❌ 绝对 `base: "/"` → 资源解析失败
- ❌ Tauri 2 release 默认不带 devtools → 白屏没法查

asset-layout-guard CI 会自动拒绝再踩这些坑的 PR。

## 测试

Rust 端 37 个单测，写在各模块的 `#[cfg(test)] mod tests` 里。新加功能尽量补单测：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

frontend 端目前用 smoke test 做"页面不崩"的兜底，没有 React 组件级单元测试 — 如果想加 Vitest，欢迎开 PR 但先开 issue 聊一下方案。

## 发版

只有维护者会做。流程见 [docs/RELEASE.md](docs/RELEASE.md)。

简版：`git tag v2.x.x && git push origin v2.x.x` → GitHub Actions 自动签名 build + 上传到 Release。

## 行为准则

技术讨论可以辩论，但人不行。互骂会被关 issue / 拒 PR。
