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
