# 发版流程（含自更新签名）

GSyncing 从 v2.1 起支持 `tauri-plugin-updater` 自更新。
本文档说明发布一个新版本时需要做的所有事 — 签名、上传、写 manifest。
首次跑前先看一遍。

---

## 一次性准备：签名密钥对

已经在 `C:\Users\loveu\.tauri\gsyncing.key` 生成过一次。
**永远不要把私钥提交到 git。** 公钥已经在 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey` 字段里写死。

如果换机器或丢了私钥：

```pwsh
npx @tauri-apps/cli signer generate --ci -w "$HOME/.tauri/gsyncing.key"
# 然后把 .pub 文件内容塞进 tauri.conf.json
# 一旦换 pubkey，老版本应用就无法验证新版本 — 用户必须手动重装一次
```

私钥也可以放到 GitHub Actions Secrets 里（推荐）以便 CI 签名。

---

## 每次发版的流程

### 1. 改版本号

```pwsh
# package.json + src-tauri/Cargo.toml + src-tauri/tauri.conf.json 三处版本同步
# 比如要发 v2.2.0：
# - package.json: "version": "2.2.0"
# - src-tauri/Cargo.toml: version = "2.2.0"
# - src-tauri/tauri.conf.json: "version": "2.2.0"
# - src/components/AboutModal.tsx: APP_VERSION 常量
```

### 2. 更新 CHANGELOG.md

写一节 `## v2.2.0 — <主题>`，列出本版改动。

### 3. 跑 release-check

```pwsh
npm run release-check
```

必须四栈全绿（build + smoke + 37 单测）。失败就不发。

### 4. 签名构建

```pwsh
# Powershell — 设置签名私钥环境变量再 build
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content "$HOME/.tauri/gsyncing.key" -Raw)
# 私钥没密码就不需要 PASSWORD 变量
npm run tauri:build
```

构建产物：
```
src-tauri/target/release/bundle/
├── msi/
│   ├── GSyncing_2.2.0_x64_en-US.msi
│   └── GSyncing_2.2.0_x64_en-US.msi.sig    # 自动产生的签名文件
└── nsis/
    ├── GSyncing_2.2.0_x64-setup.exe
    └── GSyncing_2.2.0_x64-setup.exe.sig    # 自动产生
```

### 5. 上传到 GitHub Releases

```pwsh
gh release create v2.2.0 `
  --title "v2.2.0 — <主题>" `
  --notes-file CHANGELOG-this-version.md `
  src-tauri/target/release/bundle/nsis/GSyncing_2.2.0_x64-setup.exe `
  src-tauri/target/release/bundle/nsis/GSyncing_2.2.0_x64-setup.exe.sig
```

### 6. 生成 latest.json manifest

```pwsh
# 把 .sig 文件内容 base64 一下塞进 manifest
$sig = Get-Content "src-tauri/target/release/bundle/nsis/GSyncing_2.2.0_x64-setup.exe.sig" -Raw
$manifest = @{
  version = "2.2.0"
  notes = "v2.2.0 — <主题>"
  pub_date = (Get-Date -Format "yyyy-MM-ddTHH:mm:ssZ")
  platforms = @{
    "windows-x86_64" = @{
      signature = $sig
      url = "https://github.com/ZZray/GSyncing/releases/download/v2.2.0/GSyncing_2.2.0_x64-setup.exe"
    }
  }
} | ConvertTo-Json -Depth 10
$manifest | Out-File "latest.json"
gh release upload v2.2.0 latest.json
```

**latest.json 必须挂在固定 URL** — `tauri.conf.json` 里写的是：
```json
"endpoints": [
  "https://github.com/ZZray/GSyncing/releases/latest/download/latest.json"
]
```

GitHub 的 `/latest/download/<file>` 永远指向 latest release，所以**新 release 上传 latest.json 后，所有老版本应用都会自动看到**。

### 7. 验证更新链路

1. 在另一台机器（或卸载本机版本）装老一版
2. 启动 → 应该弹出"发现新版本 v2.2.0"
3. 点"立即下载并安装" → 进度条 → 重启
4. 启动后版本号变成 v2.2.0

---

## CI 自动签名（推荐）

`.github/workflows/release.yml` 跑一份 release workflow，触发条件为 `git push --tags v*`：

```yaml
name: release
on:
  push:
    tags: ["v*"]
jobs:
  build:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: "20", cache: "npm" }
      - uses: dtolnay/rust-toolchain@stable
      - run: npm ci
      - run: npm run release-check
      - env:
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
        run: npm run tauri:build
      - uses: softprops/action-gh-release@v2
        with:
          files: |
            src-tauri/target/release/bundle/nsis/*.exe
            src-tauri/target/release/bundle/nsis/*.exe.sig
            src-tauri/target/release/bundle/msi/*.msi
            src-tauri/target/release/bundle/msi/*.msi.sig
```

把 `~/.tauri/gsyncing.key` 内容粘贴到 GitHub repo Settings → Secrets → `TAURI_SIGNING_PRIVATE_KEY`。

---

## 故障排查

- **客户端报 "InvalidSignature"** — `latest.json` 里的 signature 字段跟实际包不匹配。重新拷一遍 `.sig` 内容。
- **客户端报 "AlreadyOnLatestVersion"** — `latest.json` 的 version 字段 ≤ 客户端当前版本。检查格式（用 semver，"2.2.0" 不是 "v2.2.0"）。
- **客户端报 endpoint 404** — `tauri.conf.json` 写的 URL 不对，或者你忘了上传 latest.json。
- **下载进度卡住** — 检查 GitHub release 资产是否 public。
