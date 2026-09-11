# 代码签名申请材料（task-11）

v1.0.0 按未签名策略发布；拿到证书后接入 CI 即可自动签名（见 [RELEASING.md](./RELEASING.md)）。本文件是一份可直接复制到申请表单的材料，带 `<...>` 的字段需要你本人填写。

## 推荐路径：SignPath Foundation（开源项目免费）

DeepPi 是 MIT 许可证的公开仓库，符合 SignPath Foundation 的申请条件（开源项目、公开仓库、OSI 认可许可证）。

### 申请表单可复制内容

- **Project name**: DeepPi
- **Project website / repository**: <https://github.com/springkai66/deep-pi>
- **License**: MIT
- **Project description**:

  > DeepPi is a Windows desktop host that brings Pi Coding Agent and DeepSeek Harness (DSH) into one application: parallel Pi terminal and RPC sessions, the native DSH Web UI, a Pi package marketplace, provider and credential management, project files with Git review, and in-app runtime upgrades for Pi and DSH. It is a Tauri 2 (Rust) + Svelte 5 application distributed as NSIS and MSI installers for Windows x64.

- **Signing artifact types**: Windows PE executables and installers (`deeppi.exe`, `DeepPi_<version>_x64-setup.exe`, `DeepPi_<version>_x64_en-US.msi`)
- **Build system**: GitHub Actions (`windows-latest`), workflow `.github/workflows/release.yml`, triggered by `v*` tags
- **Expected release frequency**: `<例如：每 1-2 个月一次稳定版，必要时发布修复版>`
- **Maintainer**: springkai66 `<填写你的姓名/邮箱，用于申请审核>`
- **Additional information**: `<可补充：项目用途、目标用户、下载渠道（GitHub Releases）、是否有公司实体等>`

### 申请与接入步骤

1. 注册 [SignPath.io](https://signpath.io/) 账号，提交上述开源申请。
2. 审核通过后安装 SignPath GitHub App，并在 SignPath 中创建 DeepPi 项目与签名策略（release signing policy）。
3. 接入 CI：把 `release.yml` 中现有的 `signtool` 步骤替换为 SignPath 的签名 Action（或把待签文件作为 artifact 交给 SignPath 签名后再上传），其余流程不变。
4. 在 Release 页面更新说明，去掉 SmartScreen 提示，并补发签名版安装包。

## 备选路径

- **Azure Trusted Signing**：约 $9.99/月，需要 Azure 订阅与身份验证，审核 1-2 周；接入方式与 PFX 相同（配置 `DEEPPI_WINDOWS_CERTIFICATE_BASE64` / `_PASSWORD` 两个 Secrets）。
- **商业 CA 的 OV/EV 证书**（DigiCert、Sectigo、SSL.com 等）：OV 约 $200-600/年，需要组织实体与电话回拨，签发 1-5 个工作日；个人身份通常无法申请，建议优先前两者。

## 完成标准

- 至少提交一次申请并记录进度（申请编号、提交日期、审核状态）。
- 获得证书或签名通道后：重新打一个 `v*` tag（或补发）产出签名安装包，`signtool verify /pa /all` 通过，并更新 README/Release Notes 的签名说明。
