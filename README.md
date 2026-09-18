# DeepPi

DeepPi 是一个 Windows 桌面应用，将 Pi Coding Agent 与 DeepSeek Harness（DSH）整合到同一个工作台中。它支持多个 Pi 会话并行运行、原生 Pi TUI 与结构化对话、DSH 原生界面、项目文件管理、Git 操作、模型配置和托管运行时管理。

DeepPi 不要求用户预装 Node、Pi 或 DSH。应用会在自己的数据目录中管理运行时、凭据和会话数据，不干扰电脑上已有的开发环境。

## 下载安装

从 [GitHub Releases](https://github.com/springkai66/deep-pi/releases) 下载最新的 `DeepPi_<版本>_x64-setup.exe`（NSIS 安装包）并运行。

- 系统要求：Windows 10/11 x64。WebView2 由安装程序自动下载安装。
- 安装包目前未进行 Authenticode 代码签名，首次运行可能显示 SmartScreen 提示。请先核对下载来源，再选择「更多信息」→「仍要运行」。
- 可以在「设置 → 应用」中卸载，也可以运行安装目录中的卸载程序。

## 首次使用

1. 打开「设置 → 模型设置」，添加模型 Provider、API Key 或官方账号 OAuth 登录方式。
2. 在左侧添加并选择一个项目目录。
3. 点击「新建 Pi 任务」，选择结构化对话或原生终端模式开始工作。
4. 需要运行普通系统命令时，点击顶栏的「命令终端」，Shell 类型可以在「设置 → 通用」中选择。

Node、Pi、DSH 等托管运行时会在首次使用对应功能时按需安装。下载前会显示确认信息和目标版本。

## 当前功能

- **Pi 会话**：支持多个任务并行运行；同一会话可以在 RPC 对话模式和 Pi 原生 TUI 模式之间切换，保留完整上下文。
- **紧凑 Session 顶栏**：会话标签、标题、模型状态和用量信息集中显示，支持新建、切换、关闭和恢复会话。
- **Pi 原生 TUI**：支持光标、全屏和交互式终端操作；使用等宽代码字体，优化中文显示、字符对齐、框线和字体切换后的重排。
- **普通命令终端**：提供独立的临时 Shell 终端，支持 PowerShell、PowerShell 7、Bash 和 CMD；工作目录由当前项目决定，不写入 Pi 任务记录。
- **DSH 工作区**：在同一应用中加载 DSH 原生 Web UI，支持会话、设置和插件市场。
- **项目文件**：文件树、文件名搜索、内容搜索、只读预览、内置编辑器、差异比较和恢复副本。
- **Git 工作流**：查看变更、分类差异、暂存与取消暂存、提交、推送和远程状态核对；默认不会强制推送。
- **任务看板**：以独立窗口查看 Pi 任务状态，并执行打开、停止、重启、归档、恢复和删除等操作。
- **四象限清单**：独立 Tauri 窗口中的四象限任务清单，支持添加、勾选、删除、响应式布局和 SQLite 持久化。
- **模型与 Provider**：支持协议、Base URL、Header、模型目录、连接测试和模型代理配置；代理校验限制为 HTTPS 或本机回环 HTTP，并禁止凭据和 query。
- **官方账号 OAuth**：支持 Claude Pro/Max、ChatGPT Plus/Pro、OpenRouter、GitHub Copilot、Kimi 和 xAI 等官方登录方式；登录状态和有效期可查询，并与 Pi 原生 `auth.json` 共享凭据。
- **Pi 扩展与 MCP**：搜索、安装、更新和卸载 Pi Package，管理 MCP 服务、Skills 和工作流扩展。
- **设置与主题**：支持中英文界面、浅色/深色/跟随系统、主题包导入导出、应用字体、会话字体和代码字体设置。
- **运行时与更新**：在应用内管理 Node、Pi、DSH 和 dshmarket 运行时，支持升级、回滚和诊断。
- **安全存储**：Provider API Key 使用 Windows Credential Manager 保存；项目路径、应用设置和清单数据分别持久化在应用数据目录中。

## 下一步计划

- 完善统一代理设置：将模型请求代理、OAuth 网络代理和普通命令终端相关网络配置集中管理，增加代理连通性诊断、按 Provider 覆盖、启用状态提示和更完整的导入导出能力。
- 继续完善原生 TUI 与普通命令终端的交互体验，包括字体渲染、终端生命周期和跨平台 Shell 兼容性。
- 补充四象限清单、Provider 代理和桌面窗口行为的端到端验证。

## 已知限制（v1.0.0）

- 安装包目前未进行 Authenticode 代码签名。
- DeepPi 不读取电脑上预装的 Pi、DSH、Node 或 npm，只使用应用自己的托管运行时和配置目录。
- DSH 当前运行在已验证的 `0.1.5-rc.2`，上游版本需要经过兼容性验证后再适配。
- DSH 子界面获得焦点时，DeepPi 宿主快捷键可能暂时不生效，详见 [KEYBOARD_SHORTCUTS.md](./KEYBOARD_SHORTCUTS.md)。
- 文件内容搜索依赖系统 `PATH` 中的 `rg.exe`（ripgrep）。DeepPi 不自动安装 ripgrep；文件名搜索不受影响，详见 [TROUBLESHOOTING.md](./TROUBLESHOOTING.md)。
- 超大规模仓库、长时间稳定性和部分原生交互仍需持续验收，详见 [NATIVE_ACCEPTANCE.md](./NATIVE_ACCEPTANCE.md)。

## 开发

构建、测试、桌面开发和发布流程见 [DEVELOPMENT.md](./DEVELOPMENT.md)、[RELEASING.md](./RELEASING.md)。桌面开发使用：

```powershell
pnpm install
pnpm dev:desktop
```

验证命令：

```powershell
pnpm check
pnpm test:web
cargo check --manifest-path src-tauri/Cargo.toml
```

## 文档

- [键盘快捷键](./KEYBOARD_SHORTCUTS.md)
- [疑难排查](./TROUBLESHOOTING.md)
- [Pi RPC 兼容基线](./RPC_COMPATIBILITY.md)
- [安全说明](./SECURITY.md)
- [发布流程](./RELEASING.md)
- [English README](./README.en.md)

## 许可证

MIT
