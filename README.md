# DeepPi

DeepPi 是一款面向开发工作的桌面应用，将 [Pi Coding Agent](https://github.com/badlogic/pi-mono) 与 DeepSeek Harness（DSH）整合在同一工作台。它以项目和任务组织智能体会话，并提供文件、Git、模型及运行时管理能力。

## 功能

- **多任务协作**：并行运行 Pi 会话，在结构化对话与原生终端界面间切换；使用独立的 DSH 工作区。
- **项目工作流**：浏览、搜索和编辑文件，查看差异，暂存、提交及推送 Git 变更。
- **模型与扩展**：配置模型提供商及凭据，管理 Pi 扩展、Skills 和 MCP 服务。
- **托管运行时**：按需安装并管理 Node.js、Pi 与 DSH，无需预先配置全局环境。
- **辅助工具**：任务看板、四象限清单、主题与多语言界面。

## 安装与使用

当前提供 **Windows 10/11 x64** 版本。从 [Releases](https://github.com/springkai66/deep-pi/releases) 下载 `DeepPi_0.1.0_x64-setup.exe`，或选择 MSI 安装包。安装程序会按需获取 WebView2。安装包尚未进行 Windows 代码签名，请从本仓库的 Release 页面下载并核对来源。

首次启动后，在设置中配置模型提供商或官方账号，添加项目目录，即可创建 Pi 任务。运行时在首次需要时按提示安装；应用数据与本机已有的 Node.js / Pi / DSH 安装相互独立。

## 未来方向

- **macOS 支持**：适配桌面能力与安装发布流程。
- **微信交互**：探索通过微信接收任务通知和发起交互。
- **远程调用**：探索安全地从其他设备连接与使用工作台能力。

以上为规划方向，**不属于当前版本功能**。

## 开发

项目采用 Tauri v2、Rust、SvelteKit。安装依赖后使用 `pnpm dev:desktop` 启动桌面开发环境；构建和发布流程见 [DEVELOPMENT.md](./DEVELOPMENT.md) 与 [RELEASING.md](./RELEASING.md)。

English: [README.en.md](./README.en.md) · License: [MIT](./LICENSE)
