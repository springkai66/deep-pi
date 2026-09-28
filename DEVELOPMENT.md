# DeepPi 开发

面向贡献者的构建、测试与打包说明。用户向的安装与使用说明见 [README.md](./README.md)。

## 环境

- Windows 10/11 x64
- Node.js 22+
- pnpm 10.30.3
- Rust stable MSVC 工具链
- Microsoft C++ Build Tools
- Microsoft Edge WebView2

## 开发

```powershell
pnpm install --frozen-lockfile --ignore-scripts
pnpm dev:desktop
```

推荐统一使用 `pnpm dev:desktop` 启动桌面开发：Tauri 会通过 `beforeDevCommand: pnpm dev` 启动 Vite，再加载 `http://localhost:1420`。请保持启动终端运行。

`pnpm dev` 仅启动前端，不启动桌面宿主；不能裸启 `src-tauri/target/debug/deeppi.exe`，debug 程序依赖正在运行的 Vite 服务，否则会显示无法访问页面。

测试后恢复开发环境也须使用 `pnpm dev:desktop`，不要直接启动 debug exe。若 1420 已有服务，先在自己拥有的旧开发会话终端中按 Ctrl+C 结束该会话，避免端口冲突；不要随意结束未知进程，先确认服务归属。

## 检查

```powershell
pnpm check          # svelte-check + 类型检查
pnpm test           # 前端单测 + cargo test
pnpm build          # 前端构建
pnpm security:audit # 生产依赖漏洞审计
pnpm licenses:check # 许可证合规
```

Rust 侧也可以单独跑：

```powershell
cargo test  --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt   --manifest-path src-tauri/Cargo.toml --check
```

在装有注入式安全软件（例如会 hook `git.exe` 的防护）的机器上，整套并行跑 `cargo test` 会偶发 `git_*` 用例失败；用有界并发即可稳定：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=6
```

需要联网或耗时的用例（模型目录、隔离运行时安装/回滚、真实 Pi 会话）默认被忽略，用 `--include-ignored` 显式运行。

## 打包

```powershell
pnpm tauri build --bundles nsis        # 只出 NSIS 安装包
pnpm tauri build --bundles nsis,msi    # NSIS + MSI
pnpm tauri build --no-bundle           # 只出可执行文件
```

产物位于 `src-tauri/target/release/bundle/`。

`--no-bundle` 不是完整便携版：仍需要 WebView2 与 Pi/DSH 托管运行时。

打包后可用脚本核验：

```powershell
node scripts/release-check.mjs --require-installer
pnpm installer:smoke   # 安装 / 修复 / 卸载
```

## 稳定性测试

```powershell
pnpm perf:smoke   # 60 秒冒烟
pnpm perf:soak    # 完整的 8 小时长稳
```

脚本会采样进程树的 CPU 与内存，并在结束时断言没有残留进程。

> **重要**：长稳期间不要安装或卸载 DeepPi。NSIS 安装器会结束正在运行的 `deeppi.exe`，实测会把长稳进程直接杀掉。详见 [TROUBLESHOOTING.md](./TROUBLESHOOTING.md)。

## 发布

推送 `v*` tag 触发 `.github/workflows/release.yml`：构建、签名 updater 产物、发布 Windows 安装包并同步 `stable` 更新通道。tagged 发布必须配置 Tauri updater signing secrets；Authenticode 证书 secrets 可选，配置后自动启用代码签名。完整步骤与密钥生成方式见 [RELEASING.md](./RELEASING.md)。
