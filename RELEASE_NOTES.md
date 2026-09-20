# DeepPi v1.0.4

DeepPi 是把 Pi Coding Agent 与 DeepSeek Harness（DSH）整合到同一工作台的 Windows 桌面应用：无需预装 Node、Pi 或 DSH，运行时由应用自行托管，不干扰系统已有开发环境。

## 本版更新（v1.0.4）

- **全局网络代理设置**：设置新增「网络代理」（系统分组），支持**跟随系统 / 直连 / 手动**三种模式；手动模式填写代理地址（如 `http://127.0.0.1:7890`）与例外地址（NO_PROXY），并内置**连通性测试**（发起真实请求并显示状态码与耗时）。
- **代理覆盖到 Pi 子进程（关键修复）**：Pi RPC / TUI 会话、DSH、npm 包管理与登录流程等所有 Node 子进程都会带上代理；手动模式会自动启用 Node 的 `NODE_USE_ENV_PROXY`——此前 pi 进程内的模型请求与 Advisor 不会读取代理环境变量，这正是「Advisor consultation failed: fetch failed」的根因之一。
- **其余网络路径**：运行时（Node/Pi/DSH）下载、扩展市场、更新检查与 DeepPi 自身的模型调用一并走代理；**Provider 单独配置的代理优先于全局设置**。
- **生效时机**：新启动的任务/会话立即生效；已打开的会话需重启任务，应用内更新检查需重启应用（设置页有说明）。

## 下载安装

从 [GitHub Releases](https://github.com/springkai66/deep-pi/releases) 下载 `DeepPi_1.0.4_x64-setup.exe`（或 `DeepPi_1.0.4_x64.msi`）并运行；系统要求 Windows 10/11 x64，WebView2 由安装程序自动部署。已安装用户会通过应用内自动更新收到 v1.0.4 推送。
