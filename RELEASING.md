# DeepPi 发布流程

当前正式发布面向 Windows x64。GitHub Actions 的 `Windows Release` 工作流在推送 `v*` 标签时构建 NSIS 与 MSI 安装包，发布版本 Release，并同步 `stable` 更新通道。安装包暂未进行 Authenticode 代码签名；应用更新包使用 Tauri updater 签名。

## 发布前

在仓库的 Actions Secrets 中配置：

- `TAURI_SIGNING_PRIVATE_KEY`：Tauri updater 私钥；
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：私钥密码；
- `DEEPPI_UPDATER_PUBLIC_KEY`：对应公钥。

私钥及密码只保存在安全的本地位置和仓库 Secrets，不提交进 Git。后续版本继续使用同一密钥对，确保已安装应用能够验证更新。Windows Authenticode 证书可选：提供 `DEEPPI_WINDOWS_CERTIFICATE_BASE64` 与 `DEEPPI_WINDOWS_CERTIFICATE_PASSWORD` 后，工作流才会对安装包执行代码签名。

版本号必须在 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 和 `src-tauri/Cargo.lock` 中一致；同步更新 `RELEASE_NOTES.md`。

## 验证与发布

```powershell
pnpm install --frozen-lockfile
pnpm check
pnpm test:web
cargo test --manifest-path src-tauri/Cargo.toml
pnpm build
pnpm release:check
```

确认 CI 通过后推送主分支，再创建并推送 `v0.1.0` 标签以触发首发构建。工作流会在全部检查通过后上传 `.exe`、`.msi`、更新签名和 `latest.json`。核验版本 Release 中的安装包，以及 `stable` 通道中的 `latest.json`。后续发布只需按目标版本替换标签号并同步版本文件。

回滚 `stable` 更新通道使用 `.github/workflows/rollback.yml`；它不改变已发布版本的安装包。
