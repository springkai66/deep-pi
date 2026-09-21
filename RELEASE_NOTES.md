# DeepPi v1.0.6

DeepPi 是把 Pi Coding Agent 与 DeepSeek Harness（DSH）整合到同一工作台的 Windows 桌面应用：无需预装 Node、Pi 或 DSH，运行时由应用自行托管，不干扰系统已有开发环境。

## 本版更新（v1.0.6）

- **修复扩展市场「按下载量排序」不正确**：此前目录数据来自 npm 相关性排序的前 25 条，高下载量的包可能被挤出首页，导致「按下载量排序」后看到的也不是下载量最高的包。现在：抓取量提升到 100 条，后端固定按下载量降序返回（并列按名称），前端移除「默认排序」选项、默认即为「按下载量排序」（并列按名称稳定排序）。实测 `@cortexkit/pi-openai-auth`（7.6K/月）、`@cortexkit/pi-anthropic-auth`（6.4K/月）等此前被挤出的高下载量包现已排在前列。

## 下载安装

从 [GitHub Releases](https://github.com/springkai66/deep-pi/releases) 下载 `DeepPi_1.0.6_x64-setup.exe`（或 `DeepPi_1.0.6_x64.msi`）并运行；系统要求 Windows 10/11 x64，WebView2 由安装程序自动部署。已安装用户会通过应用内自动更新收到 v1.0.6 推送。
