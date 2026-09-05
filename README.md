# DeepPi

Windows 桌面宿主，将 Pi Coding Agent 和 DeepSeek Harness 集成到同一应用中。

## 环境

- Windows 10/11
- Node.js 22+
- pnpm 10.30.3
- Rust stable MSVC
- Microsoft C++ Build Tools
- Microsoft Edge WebView2

## 开发

```powershell
pnpm install --frozen-lockfile --ignore-scripts
pnpm tauri dev
```

## 检查

```powershell
pnpm check
pnpm test
pnpm build
pnpm security:audit
pnpm licenses:check
```

## 打包

生成 NSIS 安装包：

```powershell
pnpm tauri build --bundles nsis
```

生成 NSIS 和 MSI：

```powershell
pnpm tauri build --bundles nsis,msi
```

产物位于 `src-tauri/target/release/bundle/`。

只构建应用可执行文件：

```powershell
pnpm tauri build --no-bundle
```

这不是完整便携版，仍需要 WebView2 和 Pi/DSH runtime。

## 发布

推送 `v*` tag 后，`.github/workflows/release.yml` 会构建并发布 Windows 产物。正式发布需要配置 Tauri updater signing secrets 和 Authenticode 证书 secrets。

## 文档

- [English README](./README.en.md)
- [Security](./SECURITY.md)
- [Troubleshooting](./TROUBLESHOOTING.md)

## 许可证

MIT
