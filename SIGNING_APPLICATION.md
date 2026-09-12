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

### 资格自查（2026-09 按官方条款核对，已逐项确认）

SignPath Foundation 对 OSS 项目的硬性条件与 DeepPi 的实际情况：

| 条件 | DeepPi 现状 |
| --- | --- |
| OSI 认可的开源许可证、无商业双许可 | MIT（根目录 `LICENSE`、`package.json`、`Cargo.toml` 三处一致，`pnpm licenses:check` 通过） |
| 源码在公开仓库 | 公开仓库 `springkai66/deep-pi`（已确认 `public`、未归档） |
| 分发包可免费下载 | GitHub Releases 免费下载 |
| 由 CI 构建（不得本地手搓产物送签） | `windows-latest` 上的 `.github/workflows/release.yml`，由 `v*` tag 触发，产物经 `release-check` 校验 |
| 无恶意/PUP 行为 | 应用不收集数据、不上传遥测；凭据存 Windows Credential Manager 且不进日志 |
| 证书签发给 SignPath Foundation（其作为 publisher） | 需接受；这属于该计划的既有规则 |

### 申请与接入步骤

1. 打开 <https://signpath.io/product/open-source>（或 <https://signpath.org/apply.html>）提交开源申请；审核通常 1–3 个工作日。
2. 审核通过后安装 SignPath GitHub App，并在 SignPath 中创建 DeepPi 项目与签名策略（release signing policy）。
3. 接入 CI：把 `release.yml` 中现有的 `signtool` 步骤替换为 SignPath 的签名 Action（或把待签文件作为 artifact 交给 SignPath 签名后再上传），其余流程不变。
4. 在 Release 页面更新说明，去掉 SmartScreen 提示，并补发签名版安装包。

## 备选路径

以下信息按 2026-09 的公开资料核对，申请前请再确认一次最新政策。

- **Azure Trusted Signing（现名 Azure Artifact Signing）**：约 $9.99/月。**但存在地区限制——组织仅限美国、加拿大、欧盟、英国，个人开发者仅限美国与加拿大。中国大陆的个人开发者不符合条件**（微软官方在受限地区会把个人导向 OV 证书）。因此本路径对当前维护者**不可用**。
- **商业 CA 的 OV/EV 证书**（DigiCert、Sectigo、SSL.com 等）：OV 约 $200-600/年。多数 CA 要求**组织实体**与电话回拨，个人身份通常无法申请；若无公司实体，这条路也可能走不通。
- **结论**：对个人维护者的中国大陆主体，**SignPath Foundation（开源免费）是唯一现实可行的路径**，应作为主攻方向；若将来有公司实体，再考虑 OV。

## 完成标准

- 至少提交一次申请并记录进度（申请编号、提交日期、审核状态）。
- 获得证书或签名通道后：重新打一个 `v*` tag（或补发）产出签名安装包，`signtool verify /pa /all` 通过，并更新 README/Release Notes 的签名说明。
