## 改了什么

<!-- 一句话描述这个 PR 做了什么。比如 "在 GameLibrary 加多选+批量同步" -->

## 为什么

<!-- 解决的问题 / 关联的 issue 编号 / 用户场景 -->

Fixes #

## 怎么测的

- [ ] `npm run release-check` 本地全绿
- [ ] 手动测试过场景 X / Y / Z
- [ ] 加了对应的单测（Rust 端）

## 截图（如涉及 UI）

<!-- 拖进来，浅色 + 深色各一张更好 -->

## 红线检查

- [ ] **没有**重新引入 `vendor-*` chunks（见 docs/TAURI2-GOTCHAS.md §1）
- [ ] **没有**重新引入 `crossorigin` attribute（见 §2）
- [ ] **没有**修改 base 路径回 `/`（见 §3）
- [ ] **没有**在 async fn 里直接调 `std::fs::*`（都要 `tokio::task::spawn_blocking`）
- [ ] **没有**把跨平台敏感字段写明文（DPAPI 加密入库）

## 备注

<!-- breaking change? 配置迁移? 性能影响? 任何 reviewer 需要先知道的 -->
