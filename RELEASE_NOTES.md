# DeepPi v1.0.0

DeepPi 的第一个稳定版本：在同一个 Windows 桌面应用中使用 Pi Coding Agent 与 DeepSeek Harness。

## 主要能力

- **Pi 工作区**：多个 Pi 任务并行，原生 TUI 终端与结构化 RPC 对话可互相切换，共享同一会话。
- **DSH 原生界面**：同窗口加载 DSH Web UI，会话、设置与 dshmarket 插件市场可用。
- **项目文件**：文件树、文件名与内容搜索、只读预览、内置编辑器、差异比较与恢复副本。
- **Git 审阅**：状态分类、差异查看、显式暂存/取消暂存、提交与推送，不默认强推。
- **Pi 扩展市场**：搜索、安装、更新与卸载 Pi Package，安装前确认来源。
- **Provider 与凭据**：协议、Base URL、Header、代理与连接测试，密钥保存在 Windows Credential Manager。
- **运行时升级**：应用内升级 Pi/DSH/dshmarket，失败可回滚。

## 安装

1. 从 Releases 下载 `DeepPi_1.0.0_x64-setup.exe`。
2. 运行安装包。安装包会自动安装 WebView2。
3. 首次运行如出现 SmartScreen 提示，选择「更多信息」→「仍要运行」。

## 已知限制

- 安装包未做 Authenticode 代码签名（开源签名通道申请中）。
- 不读取本机已安装 Pi/DSH 的配置与任务列表，只使用 DeepPi 托管运行时。
- DSH 子 Webview 聚焦时仅路由 Ctrl+P / Ctrl+Shift+P / Ctrl+,（受限原生加速键，不向 DSH 页面开放 IPC）；其余快捷键需要主窗口焦点，原生验收待完成。
- v1.0.0 是首个版本，DeepPi 应用自更新的跨版本升级与回滚验证将在下个版本发布时进行。
- 8 小时长稳与部分原生交互验收尚未完成，详见仓库内 `NATIVE_ACCEPTANCE.md`。

## 反馈

请在 GitHub Issues 提交问题，并附上「设置 → 高级与诊断 → Pi RPC 诊断」导出的报告（不包含提示词、路径或凭据）。
