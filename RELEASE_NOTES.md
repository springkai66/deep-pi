# DeepPi v1.0.0

DeepPi 是把 Pi Coding Agent 与 DeepSeek Harness（DSH）整合到同一工作台的 Windows 桌面应用：无需预装 Node、Pi 或 DSH，运行时由应用自行托管，不干扰系统已有开发环境。

## 应用功能

- **Pi 智能体对话**：多个任务并行运行；同一会话可在结构化对话与 Pi 原生 TUI 之间切换并保留完整上下文；流式回复、思考过程、执行阶段（思考 / 生成回答 / 执行工具 / 等待输入）与本轮耗时实时可见，工具调用参数与输出分开展示。
- **命令终端**：独立的临时 Shell 终端，支持 PowerShell、PowerShell 7、Bash 和 CMD，工作目录跟随当前项目。
- **DSH 工作区**：在同一应用内加载 DeepSeek Harness 原生界面，支持会话与插件市场。
- **项目文件**：文件树、文件名搜索、内容搜索、只读预览、内置编辑器、差异比较和恢复副本。
- **Git 工作流**：查看变更、分类差异、暂存与取消暂存、提交、推送和远程状态核对。
- **任务看板与四象限清单**：独立窗口查看并管理 Pi 任务（打开、停止、重启、归档、删除）；四象限任务清单支持增删勾选与持久化。
- **模型与 Provider**：自定义协议、Base URL、模型目录与连接测试；API Key 保存在 Windows Credential Manager；支持 Claude、ChatGPT、OpenRouter、GitHub Copilot、Kimi、xAI 等官方账号 OAuth 登录。
- **扩展生态**：搜索、安装与卸载 Pi Package、MCP 服务、Skills 和工作流；模型参数可从 pi.dev/models 自动补全。
- **运行时与更新**：应用内管理 Node、Pi、DSH 运行时的安装、升级、回滚与诊断；应用内自动更新经签名校验。
- **外观与多语言**：简体中文 / 繁体中文 / English 界面，浅色、深色与跟随系统主题，应用、会话与代码字体可调。

## 下载安装

从 [GitHub Releases](https://github.com/springkai66/deep-pi/releases) 下载 `DeepPi_1.0.0_x64-setup.exe`（或 `DeepPi_1.0.0_x64.msi`）并运行；系统要求 Windows 10/11 x64，WebView2 由安装程序自动部署。
