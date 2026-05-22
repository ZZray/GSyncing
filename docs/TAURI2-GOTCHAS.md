# Tauri 2 + Vite 6 + WebView2 已知坑（GSyncing 实战）

> 2026-05-22 — 经过 4 轮白屏排查（v1.3.1 → v1.4.2）抓出来的真凶 + 修法。
> 未来回这个项目或新开 Tauri 2 项目时，先扫一遍这页能省好几天。

---

## 1. manualChunks 会导致 TDZ 错误（致命）

**症状**
- 装好 release，启动后白屏（HTML 加载成功，标题栏图标显示，但内容空白）
- 偶尔可复现，偶尔成功，跟 hash 随机性挂钩 — 不稳定复现是最大特征
- DevTools Console 里能看到（如果 DevTools 自动开了）：
  ```
  Uncaught ReferenceError: Cannot access 'ms' before initialization
      at vendor-other-XXXXXX.js:24:NNNNN
  ```
  变量名可能不是 `ms`，但永远是某个 vendor chunk 里的变量。

**根因**
- Vite + Rollup 用 `manualChunks` 把 `node_modules/*` 拆成多个 vendor chunk
- 某些库（比如 `ms`、`debug`、`dayjs` 内部）有循环依赖或 IIFE 初始化
- 跨 chunk 边界后，ESM 模块加载顺序被破坏 → TDZ（temporal dead zone）

**修法**
```ts
// vite.config.ts — DO NOT use manualChunks for Tauri 2 webview targets
build: {
  // 单 bundle 就单 bundle。Tauri 本地加载毫秒级，1MB JS 不是问题。
  chunkSizeWarningLimit: 2000,
}
```

**反模式（别犯）**
```ts
// ❌ 看起来"优化"的拆分，会埋雷
manualChunks(id) {
  if (id.includes("node_modules")) {
    if (id.includes("/antd")) return "vendor-antd";
    if (id.includes("/react")) return "vendor-react";
    return "vendor-other";
  }
}
```

---

## 2. Vite 默认的 crossorigin 属性

**症状**
- 类似 1，但 DevTools Console 里**完全没报错**（或仅有 CORS 相关警告）
- Network 标签里 JS 文件 200 OK 但脚本未执行
- WebView2 静默拒绝模块脚本

**根因**
- Vite 给 entry script 自动加 `crossorigin` 属性：
  ```html
  <script type="module" crossorigin src="./assets/index-XXX.js">
  ```
- Tauri 2 的 `http://tauri.localhost` 协议**不返回 CORS header**
- 严格的 WebView2 把脚本当成需要 CORS 但响应没 CORS-allow → 静默拒绝

**修法**：自定义 Vite 插件剥除属性
```ts
const stripCrossOriginPlugin = {
  name: "strip-crossorigin",
  transformIndexHtml(html: string) {
    return html.replace(/\s+crossorigin(?=[ >])/g, "");
  },
};

export default defineConfig({
  plugins: [react(), stripCrossOriginPlugin],
  // ...
});
```

---

## 3. base 路径必须是相对的

**症状**
- 类似 1/2，开发模式 OK，生产模式白屏
- Network 里 .js 文件 404

**根因**
- Vite 默认 `base: "/"`，HTML 里生成 `/assets/index-XXX.js`
- Tauri 2 webview 的 URL 是 `http://tauri.localhost/index.html`
- 浏览器把 `/assets/...` 解析成 `http://tauri.localhost/assets/...` — 通常 OK
- 但某些 webview 版本或自定义 scheme 配置会出问题

**修法**
```ts
export default defineConfig({
  base: "./",  // 相对路径最稳
  // ...
});
```

---

## 4. DevTools 在 release 默认关闭

**症状**
- 白屏 + F12 没反应 + 右键没"检查"菜单
- 无法看 Console / Network 排查

**根因**
- 默认 Tauri release build 不带 DevTools feature
- 即使带了，可能也没自动打开

**修法**
1. `Cargo.toml`:
   ```toml
   tauri = { version = "2.1", features = ["tray-icon", "devtools"] }
   ```
2. setup hook 里**条件**自动 open：
   ```rust
   if state.get_settings_blocking().auto_open_devtools {
       if let Some(win) = handle.get_webview_window("main") {
           win.open_devtools();
       }
   }
   ```
3. UI 里加 toggle，让用户验证 OK 后能关掉

---

## 5. 防御性 HTML：boot 加载页 + 早期错误捕获

让用户在 JS 真正跑起来之前，至少看到点东西。`index.html` 里：

```html
<body>
  <div id="root">
    <!-- 在 React 接管前先显示 -->
    <div id="boot">
      <div class="logo">G</div>
      <div>GSyncing 启动中</div>
      <pre id="boot-err" style="display:none"></pre>
    </div>
  </div>
  <script>
    // window-level error / rejection 捕获
    var pre = document.getElementById("boot-err");
    function show(msg) { pre.style.display="block"; pre.textContent=msg; }
    window.addEventListener("error", e => show(e.error?.stack || e.message));
    window.addEventListener("unhandledrejection", e => show(e.reason?.stack || e.reason));
    setTimeout(() => {
      if (document.getElementById("boot")) {
        show("Bundle 仍未加载，按 F12 看 Network");
      }
    }, 10000);
  </script>
  <script type="module" src="/src/main.tsx"></script>
</body>
```

React 端再加 `ErrorBoundary` 兜底渲染异常。

---

## 6. 图标必须是真正的 ICO 多尺寸 BMP-payload

**症状**
- `cargo tauri build` 阶段 RC.EXE 报 `error RC2175: not in 3.00 format`

**根因**
- 直接把 PNG 改后缀为 .ico 不行
- 老 RC.EXE 不识别 PNG-payload ICO（modern Windows 支持，但 RC 编译器不）

**修法**
- 用 BMP-payload ICO（每个 size 一份 BITMAPINFOHEADER + RGBA pixels + AND mask）
- 至少含 16/32/48/64/128/256 六种尺寸覆盖所有 Windows 显示场景
- 项目里有现成的 Node 生成脚本可以复用

---

## 7. WebView2 strict mode 跟 Chrome dev 行为不一致

哪怕你 Chrome 测试好好的，Tauri 2 webview 可能挂。**永远要测 release build**，不能只靠 `npm run tauri:dev`。

具体差异：
- 更严的 CORS（见 2）
- 更严的 mixed content
- 偶尔的 ESM 模块加载顺序差异（见 1）

---

## 验证清单（每次 release 前过一遍）

```bash
# 1. cargo check 干净
cargo check --manifest-path src-tauri/Cargo.toml

# 2. Rust 单测全过
cargo test --manifest-path src-tauri/Cargo.toml --lib

# 3. tsc 严格通过
npx tsc -b --force

# 4. vite 构建无错
npm run build

# 5. 检查 HTML 里没 crossorigin
grep crossorigin dist/index.html  # 应该 0 行

# 6. 检查 base 是相对的
grep -E "src=|href=" dist/index.html | grep -v "\./"  # 应该 0 行

# 7. 检查没拆 chunk
ls dist/assets/ | grep "vendor-"  # 应该 0 行

# 8. 真实 release build
npm run tauri:build

# 9. 装产物试一遍，重点确认非白屏
```

---

## 致谢

- [Tauri 2 official docs](https://tauri.app/v2/)
- [Vite asset handling docs](https://vitejs.dev/guide/assets)
- 4 轮 v1.3.x 排查的真实截图证据
