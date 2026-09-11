# DeepPi 发布流程

v1.0 采用「未签名安装包 + 更新签名」策略发布：Tauri updater（minisign）密钥是 tagged 发布的必需项；Authenticode 代码签名在拿到证书前跳过，配置 Secrets 后自动启用。

## 必需的仓库 Secrets

在 GitHub 仓库 `Settings → Secrets and variables → Actions` 中配置：

| Secret | 必需 | 说明 |
| --- | --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | 是 | updater minisign 私钥内容 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 是 | 生成密钥时设置的密码 |
| `DEEPPI_UPDATER_PUBLIC_KEY` | 是 | minisign 公钥内容，用于 tagged 构建内嵌更新公钥 |
| `DEEPPI_WINDOWS_CERTIFICATE_BASE64` | 否 | Authenticode PFX 的 Base64；配置后自动签名与验签 |
| `DEEPPI_WINDOWS_CERTIFICATE_PASSWORD` | 否 | PFX 密码，与上一个 Secret 同时配置 |

### 生成 updater 密钥

```powershell
pnpm tauri signer generate -w "$HOME\.deeppi-updater.key" -p "<强密码>" --ci
```

- `TAURI_SIGNING_PRIVATE_KEY` 填 `$HOME\.deeppi-updater.key` 文件的完整内容。
- `DEEPPI_UPDATER_PUBLIC_KEY` 填命令输出的公钥（同时写入 `$HOME\.deeppi-updater.key.pub`）。
- 私钥只保存在本地与仓库 Secrets，不提交到仓库；泄露后需要重新生成并发布新版本。

### 启用 Authenticode（拿到证书后）

```powershell
[Convert]::ToBase64String([IO.File]::ReadAllBytes("certificate.pfx")) | Set-Clipboard
```

把 Base64 写入 `DEEPPI_WINDOWS_CERTIFICATE_BASE64`，PFX 密码写入
`DEEPPI_WINDOWS_CERTIFICATE_PASSWORD`。下次 tagged 发布将自动执行 signtool
签名与验签；未配置时 workflow 会打印未签名提示并继续发布。

### tagged 路径已本地 dry-run 验证

用一次性密钥把 tagged 路径整条跑通过（签名配置生成 → 签名构建 → updater 清单 → 清单校验，全部 PASS），因此配置完 Secrets 后打 tag 不应再出现流程性失败。注意 `scripts/prepare-updater-config.mjs` 与 `scripts/prepare-updater-manifest.mjs` 都依赖 `GITHUB_REPOSITORY`（CI 自动注入）来推导 `stable` 更新源；本地手动复现时需显式设置该环境变量，否则会以「DEEPPI_UPDATER_ENDPOINT is required」失败。

## 发布步骤

1. 更新版本号（三处保持一致）：`package.json`、`src-tauri/tauri.conf.json`、
   `src-tauri/Cargo.toml`。
2. 本地验证：`pnpm check && pnpm test:web && cargo test --manifest-path src-tauri/Cargo.toml`，
   以及 `node scripts/release-check.mjs`。
3. 提交并推送 `main`，确认 CI 通过。
4. 打 tag 并推送：`git tag v1.0.0 && git push origin v1.0.0`。
5. 观察 `Windows Release` workflow：构建 NSIS/MSI、签名 updater 产物、发布
   Release、同步 `stable` 通道。
6. 从 Release 下载安装包复核，并确认 `stable` 通道的 `latest.json` 可访问。

## 回滚

`.github/workflows/rollback.yml` 用于回滚 `stable` 更新通道（详见 workflow 输入参数）。应用内回滚由运行时组件级回滚机制处理。

## 申请代码签名证书（非发布门槛）

v1.0 先发未签名安装包；拿到证书后按下面的方式接入，下次 tagged 发布会自动签名。可直接复制的申请表单在 [SIGNING_APPLICATION.md](./SIGNING_APPLICATION.md)。

1. **SignPath.io（开源项目免费）**：DeepPi 是 MIT 许可证的公开仓库，符合 SignPath Foundation 申请条件。流程：注册 → 提交开源申请（项目、仓库、许可证、用途）→ 审核通过后安装 SignPath GitHub App 并配置签名策略；审批通常数天到 2 周。
2. **Azure Trusted Signing**：约 $9.99/月，需要 Azure 订阅与身份验证，审核 1–2 周，适合无法走开源通道的小团队。
3. **商业 CA 的 OV/EV 证书**（DigiCert、Sectigo、SSL.com 等）：OV 约 $200–600/年，需要组织实体与电话回拨，签发 1–5 个工作日；EV 审核更严。个人身份通常只能走前两者。

拿到证书后：使用 PFX 时按“启用 Authenticode”配置两个 Secrets 即可；使用 SignPath 时由 SignPath 的 GitHub Action 完成签名，可替换 workflow 中的 signtool 步骤。
