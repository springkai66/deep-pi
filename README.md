# DeepPi

Windows 桌面宿主，将 Pi Coding Agent 和 DeepSeek Harness (DSH) 集成到同一应用中：多任务 Pi 终端与 RPC 对话、DSH 原生 Web UI、扩展市场、Provider 与凭据管理、项目文件与 Git 审阅，以及 Pi/DSH 运行时的应用内升级。

## 下载安装

从 [GitHub Releases](https://github.com/springkai66/deep-pi/releases) 下载最新的 `DeepPi_<版本>_x64-setup.exe`（NSIS 安装包）。

- 系统要求：Windows 10/11 x64；WebView2 由安装包自动下载安装。
- v1.0.0 安装包尚未做 Authenticode 代码签名，首次运行可能出现 SmartScreen 提示：选择「更多信息」→「仍要运行」。请先核对下载来源。
- 卸载通过「设置 → 应用」或在安装目录运行卸载程序完成。

## 环境（开发）

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

推送 `v*` tag 后，`.github/workflows/release.yml` 会构建、签名（可选）并发布 Windows 产物，同时同步 `stable` 更新通道。tagged 发布必须配置 Tauri updater signing secrets；Authenticode 证书 secrets 可选，配置后自动启用签名。步骤与密钥生成方式见 [RELEASING.md](./RELEASING.md)。

## 已知限制（v1.0）

- 安装包未做 Authenticode 代码签名；开源签名通道申请中。
- 不读取本机安装的 Pi/DSH/Node/npm；只使用 DeepPi 自带的托管运行时与配置目录（Node 由应用从官方发行包安装并校对 SHA-256）。
- DSH 固定为已验证的 `0.1.1-rc.2`：上游 0.1.5-rc.1 改变了本地认证方式，适配前不提供升级（界面会说明原因）。
- DSH 子 Webview 获得焦点时宿主快捷键不生效（见 [KEYBOARD_SHORTCUTS.md](./KEYBOARD_SHORTCUTS.md)）。
- DeepPi 应用自身的自动更新管道已配置（updater 签名与 `stable` 通道），但 v1.0.0 是首个版本，跨版本自更新与回滚的真实验证将在下一个版本发布时进行。
- 8 小时长稳、超大规模仓库与部分原生交互验收尚未完成（见 [NATIVE_ACCEPTANCE.md](./NATIVE_ACCEPTANCE.md)）。

## 文档

- [键盘快捷键](./KEYBOARD_SHORTCUTS.md)
- [Pi RPC 兼容基线](./RPC_COMPATIBILITY.md)
- [原生验收清单](./NATIVE_ACCEPTANCE.md)
- [发布流程](./RELEASING.md)
- [English README](./README.en.md)
- [Security](./SECURITY.md)
- [Troubleshooting](./TROUBLESHOOTING.md)
- [UI 重设计计划与实施记录](./UI_REDESIGN_PLAN.md)

## 许可证

MIT
