# DeepPi

Windows 桌面宿主，把 Pi Coding Agent 和 DeepSeek Harness (DSH) 集成到同一个应用里：并行运行多个 Pi 任务（原生终端或结构化对话）、使用 DSH 原生界面、浏览与编辑项目文件、审阅 Git 变更与提交推送、安装 Pi 扩展、管理模型凭据，并在应用内升级运行时。

不需要预装 Node、Pi 或 DSH —— 运行时由应用自己管理，不与电脑上已安装的环境互相影响。

## 下载安装

从 [GitHub Releases](https://github.com/springkai66/deep-pi/releases) 下载最新的 `DeepPi_<版本>_x64-setup.exe`（NSIS 安装包），双击安装。

- 系统要求：Windows 10/11 x64。WebView2 由安装包自动下载安装。
- 安装包尚未做 Authenticode 代码签名，首次运行可能出现 SmartScreen 提示：选择「更多信息」→「仍要运行」。请先核对下载来源。
- 卸载：在「设置 → 应用」中卸载，或运行安装目录里的卸载程序。

## 首次使用

1. **配置模型凭据**：打开「设置 → 模型设置」，填入 Provider 的 API Key（密钥保存在 Windows Credential Manager，不写入配置文件）。
2. **打开项目**：在左栏点「添加项目目录」选择代码目录。
3. **新建任务**：点「新建 Pi 任务」创建 Terminal 或对话任务，即可与 Pi 协作。

首次使用某个功能时，应用会按需从官方发行包安装托管运行时（Node、Pi、DSH），安装前的确认框会说明将要下载的版本。

## 主要功能

- **Pi 任务**：多个任务并行；终端模式与对话模式可随时互相切换，且共享同一会话，不丢上下文。
- **DSH 工作区**：在同一窗口内加载 DSH 原生 Web UI，会话、设置与插件市场可用。
- **文件**：文件树、文件名与内容搜索、只读预览、内置编辑器、差异比较与恢复副本。
- **Git**：变更分类、差异查看、显式暂存/取消暂存、提交、推送与远程核对；不会默认强推。
- **Pi 扩展**：搜索、安装、更新、卸载 Pi Package，安装前确认来源与版本。
- **模型凭据**：协议、Base URL、Header、代理与连接测试。
- **运行时与更新**：应用内升级 Node / Pi / DSH / dshmarket，保留上一版本以便回滚。

## 已知限制（v1.0）

- 安装包未做 Authenticode 代码签名；开源签名通道申请中。
- 不读取本机安装的 Pi/DSH/Node/npm；只使用 DeepPi 自带的托管运行时与配置目录（Node 由应用从官方发行包安装并校对 SHA-256）。
- DSH 运行在已验证的 `0.1.5-rc.2`：宿主已适配其 launch-token 认证与 `/api/<ns>/<method>` RPC 协议；上游更新仍需通过兼容验证后才会提供。
- DSH 子界面获得焦点时，宿主快捷键不生效（见 [KEYBOARD_SHORTCUTS.md](./KEYBOARD_SHORTCUTS.md)）。
- 文件侧栏的**内容**搜索依赖系统 `PATH` 上的 `rg.exe`（ripgrep）：应用不自带也不自动安装，未安装时该模式会提示「未找到 ripgrep」，文件名搜索不受影响（见 [TROUBLESHOOTING.md](./TROUBLESHOOTING.md)）。
- DeepPi 自身的自动更新管道已配置，但当前发布到 v1.0.3；跨版本自更新与回滚的真实链路验证随 1.0.3 发布进行。
- 8 小时长稳、超大规模仓库与部分原生交互验收尚未完成（见 [NATIVE_ACCEPTANCE.md](./NATIVE_ACCEPTANCE.md)）。

## 开发

构建、测试与发布流程见 [DEVELOPMENT.md](./DEVELOPMENT.md) 与 [RELEASING.md](./RELEASING.md)。

## 文档

- [键盘快捷键](./KEYBOARD_SHORTCUTS.md)
- [疑难排查](./TROUBLESHOOTING.md)
- [Pi RPC 兼容基线](./RPC_COMPATIBILITY.md)
- [安全说明](./SECURITY.md)
- [发布流程](./RELEASING.md)
- [English README](./README.en.md)

## 许可证

MIT
