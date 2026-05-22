# Changelog

所有版本均经过 `cargo check` + `cargo test --lib` + `npx tsc -b` + `npm run build` 四栈编译验证。

## v2.1.0 — 自更新（带签名校验）

- **tauri-plugin-updater 接入**：内置增量更新检查，从配置的 endpoint 拉 `latest.json`，verify 签名后下载 + 安装 + relaunch
- **签名密钥对生成**：minisign 风格的 ed25519，私钥落本地 `~/.tauri/gsyncing.key`，公钥 embed 在 `tauri.conf.json`。没有匹配私钥的包 client 拒绝安装
- **关于对话框加"检查更新"按钮**：点击 → 立即查 endpoint → 弹出版本说明 + 进度条 + 安装重启
- **启动时静默自检**：Settings → "启动时自动检查更新"（默认开）。后台 fetch latest.json，有新版本就弹出 About 对话框
- **endpoint = GitHub Releases**：`https://github.com/ZZray/GSyncing/releases/latest/download/latest.json`，永远指向最新 release
- **docs/RELEASE.md**：详细发版流程（签名 / 上传 / latest.json 格式 / CI workflow 模板 / 故障排查）

## v2.0.0 — CI 基础设施 + 托盘与冲突 UX

- **GitHub Actions CI**：`.github/workflows/ci.yml` 两个 job：
  - `release-check`（Windows）：build + smoke + cargo check + cargo fmt + cargo test，clippy 非阻塞
  - `asset-layout-guard`（Linux）：grep `vendor-*` chunks / `crossorigin` attribute / 绝对 `/assets/` 路径 — 任何踩 v1.3 白屏坑的 PR 立刻 fail
- **托盘"最近游戏"快捷项**：tray 菜单根据 last_sync_at 自动列出最近 5 个游戏作"同步「X」"项。一键即同步特定游戏，不用展开窗口
- **冲突 Modal 批量应用**：Ask 模式下顶部多了三个 Radio.Button：「全部保留本地 / 全部保留远端 / 全部保留双方」，一键改全部冲突的选择，避免一个一个点
- 截至本版本：**80 个任务 / 37 单测 / 11 个迭代版本（v0.1 → v2.0）**，整个从"白屏排查"到"产品级完整"的旅程闭合

## v1.9.0 — 配置体验打磨

- **路径验证按钮**：GameEditor 加「验证路径」按钮，调新 `validate_game_paths` 命令立即扫描，提示"扫到 N 个文件，共 X MB"。配错环境变量 / glob 写错可立即发现
- **快照排序选项**：除原有按时间倒序外，加最早优先 / 文件多→少 / 体积大→小 / 名字 A-Z 共 5 种排序
- **带宽限制改 MB/s 按钮组**：原来"字节/秒"输入框（1048576 是多少？）改成 7 档预设按钮（不限 / 512 KB / 1 MB / 2 / 5 / 10 / 50 MB），右侧显示当前换算值

## v1.8.0 — 存储分析 + 批量操作 + 游戏置顶

- **存储用量分析**：SyncStatus 页顶部加堆叠条形图，按字节占比可视化每个游戏的本地存档大小（含 hover tooltip + 图例）
- **游戏置顶**：卡片右上角图钉按钮（点亮 = 已置顶）。置顶的游戏始终排在最前，不论当前 sort 选什么
- **批量同步**：卡片左上角 hover/选中 时显示复选框。多选后头部出现"批量同步 (N)"按钮 + 下拉（同步 / 推送 / 拉取 / 清除选择）
- **per-game 锁保证串行**：批量同步并发触发 N 个 sync，engine 的 per-game Mutex 自动按 game 串行化，避免数据竞争

## v1.7.0 — 深色模式 + 冲突真交互

- **深色模式**：Settings 加"界面主题"下拉（🌞 浅色 / 🌙 深色 / 🖥️ 跟随系统）。即时生效无需重启。CSS 用 `data-theme` 属性 + 变量化所有 sidebar/header/content/log 颜色
- **冲突 Ask 策略真正实现**（之前偷偷降级到 newer-wins）：当 ConflictPolicy=Ask 且预览出现冲突时，SyncPreviewModal 的冲突 Tab 给每个文件一组 Radio（保留本地 / 保留远端 / 保留双方）。"执行同步"按钮调新命令 `sync_with_overrides` 把用户选择透传给引擎
- **engine override 支持**：`decide_conflict` 增加 overrides 参数，per-file override 优先于 policy
- **新单测**：`decide_override_beats_policy` 验证 override 路径，覆盖 v0.6 留下的"Ask 未实现"漏洞
- 测试总数：v1.6 36 → **v1.7 37**

## v1.6.0 — 视觉分类 + 预设扩充 + 数据安全

- **游戏卡片按品类着色**：RPG 蓝紫 / Action 红橙 / Strategy 翠绿 / Roguelike 紫粉 / Sandbox 黄绿 / Other 灰。左下角加品类小标签
- **预设扩充到 34 个**：v1.6 新加 Phasmophobia / RDR2 / GTA V / Stellaris / 老滚 4 / 辐射 4 / Persona 5R / RE2 / 死亡搁浅 / Ori / Slay the Spire
- **GameProfile 加 `category` 字段**：编辑器加品类下拉（带 emoji），自定义游戏也能选
- **restore 二次确认升级**：从 Popconfirm 改成显式 Modal，列出"将覆盖 X 文件 / Y 字节"+"操作可逆"说明，符合数据破坏性操作规格
- **自定义品类**：用户在 GameEditor 可自由选 RPG / 动作魂系 / 策略 / Roguelike / 沙盒生存 / 其它

## v1.5.0 — 界面打磨 + 易用性增强

- **关于对话框**：侧栏底部 "v0.1.0 · 关于" 入口，弹窗显示版本 / License / 数据目录 / GitHub / 安全说明
- **顶部 header 信息块**：N 游戏 · M 后端 · 上次同步 X 分钟前，淡灰底色，一眼掌握状态
- **键盘快捷键**：Ctrl+S 一键同步 / Ctrl+L 日志页 / Ctrl+, 设置页 / Ctrl+/ 关于
- **游戏库空状态升级**：大号灰阶 G logo + 引导文案 + 主 CTA 按钮 + "23 款预设"提示
- **预设按品类分组**：🎭 RPG / ⚔️ 动作魂系 / 🎯 策略 / 🎲 Roguelike / 🌍 沙盒生存 / 🎮 其它，下拉框支持搜索
- **release-check 流水线**：`npm run release-check` 一键过 build + smoke + Rust 单测，发版前必跑
- **TAURI2-GOTCHAS.md** 详细文档化 4 轮白屏踩坑

## v1.4.2 — 白屏第四次（也是最终）修复

- **真凶**：vendor-other 拆出来的 chunk 抛 `ReferenceError: Cannot access 'ms' before initialization`（TDZ 错误）。`ms` 这个时间格式化包被 `dayjs` / `debug` 间接依赖，跨 chunk 边界后 ESM 模块初始化顺序被破坏。crossorigin attribute 是真问题但只是表象，manualChunks 才是 Tauri 2 webview 下持续白屏的根本原因
- **修法**：彻底废 manualChunks，回到单 bundle（1.17 MB）。Tauri 本地加载毫秒级，没必要省 KB
- **保留**：crossorigin strip plugin / index.html boot 页 / ErrorBoundary / autoOpenDevtools 设置 / 24+ 个游戏预设 / 卡片存档大小 / 搜索排序

## v1.4.1 — 游戏库扩充 + 视觉打磨

- **预设游戏库扩到 23 个**：原神 / 星穹铁道 / 巫师 3 / Cyberpunk 2077 / Elden Ring / BG3 / Skyrim / Disco Elysium / Dark Souls 1-3 / Sekiro / Hollow Knight / Civilization VI / Factorio / Hades / Minecraft / Stardew Valley / Don't Starve Together / RimWorld / Terraria / Oxygen Not Included / KSP，覆盖 RPG / Action / Strategy / Roguelike / Sandbox 五大类。GameEditor 和 OnboardingWizard 共享同一份目录（`src/data/gamePresets.ts`）
- **游戏卡片显示本地存档大小 + 文件数**："5.2 MB · 12 文件"。后端 `compute_save_size` 命令复用 scanner，前端 30s TTL 缓存
- **游戏库搜索 + 排序**：按名字过滤；按添加顺序 / A-Z / 最近同步三种排序

## v1.4.0 — 白屏根因修复 + 调试基础设施

- **白屏根因**：Vite 默认在 entry script 加 `crossorigin` 属性，Tauri 2 的 `http://tauri.localhost` 协议不返 CORS header，WebView2 静默拒绝执行模块脚本。**修法**：自定义 Vite 插件 `transformIndexHtml` 把 `crossorigin` 属性剥除
- **manualChunks 恢复**：CORS 修了后，vendor 拆分重新可用。主入口 JS 1.16 MB → 55 KB
- **DevTools 改 setting**：v1.3.x 的强制 auto-open 改成 `autoOpenDevtools` 用户开关（首次默认 ON 供白屏诊断，验证 OK 后可关）
- **日志落盘**：app 退出时 `LogBus.persist_to_disk()` 自动 flush。LogViewer 加"导出日志"按钮按需 flush + "打开数据目录"一键 Explorer 打开
- **index.html boot 加载页**：在 JS 解析前先显示 G logo + 弹跳点 + 错误捕获，10s 超时自动显示红色诊断文字
- **正经的 G logo 图标**：6 尺寸 ICO（16/32/48/64/128/256），蓝紫渐变 + 白色 G

## v1.3.0 — 易用性最后一公里

- **stats.jsonl 自动滚动截断**：> 5000 条时砍掉最早 1000 条。长期使用文件不会无限增长
- **系统通知**：同步完成 / 失败发桌面通知（tauri-plugin-notification）。Settings 加开关，默认开启。最小化到托盘也看得到
- **真实 release build**：v1.3 MSI + NSIS 安装包出产

## v1.2.0 — 多机一致性 + 可观测性

- **远端锁**：`.gsyncing/lock.json` 持锁机器名 + 时间戳 + nonce，TTL 5min。另一台机器同步同一游戏时会看到"机器 X 正在同步"的可读错误，不会盲目并发写。锁带 heartbeat（每 2.5min 续约），长同步不被 steal
- **同步统计仪表板**：SyncStatus 页加 4 张汇总卡（累计次数 / 成功 / 失败 / 累计字节）+ 最近 50 次同步表格。后端 append-only JSONL 存到 `data_dir/stats.jsonl`
- 测试总数：v1.1 33 → **v1.2 36**（含 remote_lock + stats）

## v1.1.0 — 性能 + 状态可见性

- **WebDAV 原生 COPY**：自己用 reqwest 发 COPY + Destination header（RFC 4918），版本化时不再 get+put 双倍带宽。服务器不支持时自动 fallback，并标记不再尝试
- **全局带宽限制**：token bucket（rclone `--bwlimit` 风格）。Settings 调 `maxBytesPerSec`，0 = 无限
- **游戏卡片实时速度徽章**：同步中卡片封面右上角显示百分比 + phase 箭头（↑↓📌⏮）
- **snapshot 模块单测**：9 个新测试覆盖 sanitize 边界 / 序列化 / camelCase
- 测试总数：v1.0 24 → **v1.1 33**

## v1.0.0 — 收敛发布

- LICENSE（MIT）+ CHANGELOG + 重写 README
- `cargo fmt` 格式化全代码库
- 真实 release build 验证（`npm run tauri:build` 产出 .msi / .exe）
- 24 个 Rust 单元测试，覆盖：路径展开、ASCII 大小写无关替换、SHA 缓存复用、glob 过滤、AppError 序列化、冲突决议矩阵、push/pull plan、DPAPI round-trip、base64

## v0.8 — 收敛冲刺

- **首次启动引导**：3 步 wizard（选后端类型 → 填凭据 + 测试连通 → 添加第一个游戏含预设）
- **Vite manualChunks 拆分**：主入口 bundle 从 1.1 MB → 42 KB；antd / react / dayjs / zustand 各拆独立 vendor chunk
- **Rust 核心单测**：scanner / paths / crypto / engine planning / decide_conflict 矩阵 / push & pull plan

## v0.7 — 安全 + 可移植性

- **凭据加密**（Windows DPAPI）：S3 AccessKey / WebDAV 密码不再明文落盘。绑定当前 Windows 用户账户
- **配置导入/导出**：迁移到新机器一键打包。可选"含 / 不含凭据"
- **自动版本单条管理**：版本抽屉每行 恢复 / 导出 / 删除 三按钮

## v0.6 — 命名快照（多结局存档）

- **命名快照**：游戏卡片菜单"标记当前为快照"打标签如「黑暗剧情线-决战前」。自动版本按 N 滚动，命名快照永不被清理
- 抽屉双 Tab：自动版本 / 命名快照独立列表
- restore 自动归档当前 live（可逆）+ 更新 prior_index 防止下次同步触发冲突

## v0.5 — 速度 / 取消 / per-game 后端

- **传输速度 + ETA**：5s 滑动窗口算瞬时速率 + 剩余时间
- **取消同步**（CancellationToken）：进度条右侧"取消"按钮，rclone 风格"完成当前文件后停队列"
- **per-game 后端 override**：不同游戏可路由到不同云

## v0.4 — 进度 / 托盘 / 流式

- **实时进度事件**：每个文件完成 emit `sync-progress`，前端进度条 + tooltip
- **系统托盘**：关闭窗口最小化到托盘，文件监控 / 自动同步 / 进程检测后台继续；托盘菜单"立即同步全部 / 显示窗口 / 退出"
- **S3 大文件流式**：>64 MiB 用 `ByteStream::from_path` 流式上传 / 下载，内存与文件大小解耦

## v0.3 — UX 增强

- **Dry-run 预览**（FreeFileSync 风格）：手动同步前弹 modal 显示"将上传 N / 下载 M / 删除 K / 冲突 J"
- **版本浏览 / 回滚**：抽屉按文件分组版本列表，一键 Popconfirm 回滚（自动归档当前版本）
- **进程退出触发**（Windows）：sysinfo 每 5s 轮询，配置 `processName` 的游戏退出时自动 Push

## v0.2 — 性能 + 策略升级（FreeFileSync 对齐）

- **mtime+size 预过滤**：复用上次 SHA 跳过重哈希。第二次同步典型 10-100× 提速
- **并发上传 / 下载**：`buffer_unordered(N)` 默认 4
- **指数退避重试**：transient 5xx / 网络错误自愈（500ms / 2s / 8s × 4）
- **版本保留**（Syncthing 风格）：远端覆盖前归档到 `.gsyncing/versions/<rel>.<ms>`，默认保留 5 个
- **rename-on-conflict**：冲突默认保留双方（`.local-<ts>` / `.remote-<ts>`）

## v0.1 — 首发可用

- Tauri 2 + React 18 + Antd 5 + Rust + TS 项目骨架
- StorageBackend trait + S3（兼容腾讯云 COS / 阿里云 OSS）+ WebDAV 实现
- 同步引擎：扫描 + SHA-256 哈希 + 远端 diff + 双向 sync
- 文件监控（notify + 5s 去抖）+ 周期同步
- UI：游戏库 / 同步状态 / 云存储设置 / 日志查看
- 内置预设：原神 / 巫师 3 / 黑暗之魂 3 / Stardew Valley
- 双审流程通过：opus 一审 8 Critical → 全修 → sonnet 二审通过
