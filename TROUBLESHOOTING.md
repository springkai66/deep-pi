# DeepPi 故障排查

## 启动后窗口空白或无法连接

确认已安装 Microsoft Edge WebView2。正式安装包使用 WebView2 download bootstrapper；离线环境需要提前安装 Evergreen WebView2 Runtime，然后重新启动 DeepPi。

开发环境先运行：

```powershell
pnpm install --frozen-lockfile --ignore-scripts
pnpm tauri dev
```

如果只看到 localhost 连接错误，确认 Vite 监听 `127.0.0.1:1420`，并重启开发宿主。

## Pi 无法启动

DeepPi 优先读取 `%LOCALAPPDATA%\com.deeppi.desktop\runtimes\pi\active.json` 中的托管运行时指针，新版本位于同级 `versions` 目录。没有指针文件的旧安装继续使用 `current`。设置页显示 `system` 时，应用正在回退到 PATH 中的 `pi`，这只适合开发环境。指针损坏或指向缺失的运行时会报错，不会静默切换到本机 Pi。

确认以下目录可写，并检查日志：

```text
%APPDATA%\com.deeppi.desktop\agents\pi
%LOCALAPPDATA%\com.deeppi.desktop\runtimes\pi
%LOCALAPPDATA%\com.deeppi.desktop\logs
```

不要把 API Key、完整提示词或工具参数写进问题报告。

## DSH 页面打不开

DSH 只绑定 `127.0.0.1` 动态端口。先在设置中停止并重新启动 DSH；如果仍然失败，确认没有安全软件阻止 DeepPi 启动托管 Node。不要把 DSH 端口改为 `0.0.0.0`，否则会扩大本地服务的暴露面。

DSH 的 profile 位于：

```text
%APPDATA%\com.deeppi.desktop\agents\dsh
```

如果更新后市场不兼容，使用设置页的“修复”或“回滚”。

## 更新失败

更新前 DeepPi 会检查活动任务。Pi/DSH 安装到独立版本目录，验证后原子替换 `active.json`；旧版本保留供回滚，不再搬移正在使用的 `current`。旧安装的 `backups` 仍兼容。先停止对应 Pi Session 或 DSH，再重试。更新失败时不要手动删除 `active.json`、`versions` 或 `backups`，可使用设置页的“回滚”。

运行时和扩展操作进行中可请求取消。取消被后端接受后，界面会等待进程结束及必要的恢复完成；已经进入提交阶段的操作不能再取消。网络探测和文件复制阶段可能需要等到下一检查点。

扩展与 dshmarket 的原地修改会先建立持久化快照。应用意外退出后，下次启动会在创建会话前恢复未提交快照。若提示 `pending package recovery`，请关闭并重新启动 DeepPi；恢复失败时保留日志及 `backups\operation-*`，不要继续修改包目录。旧格式快照只保留用于人工诊断，不自动重放。

没有网络时，设置页可能显示带“离线缓存”的旧检查结果；旧结果最多保留 7 天，不代表当前 registry 状态。需要代理时，在启动 DeepPi 前设置 `HTTPS_PROXY` 或 `HTTP_PROXY`；代理必须是 HTTPS，或指向 loopback 的 HTTP 地址。

```powershell
$env:HTTPS_PROXY = "https://proxy.example.com:8443"
pnpm tauri dev
```

## DeepPi 自身更新失败

正式 tagged Release 通过 Tauri updater 使用签名的 `latest.json` 和 Windows 安装包。更新入口位于设置页的“应用更新”；开发构建没有 updater endpoint 时显示“不可用”，不会安装未签名文件。

检查发布产物：

```powershell
$env:DEEPPI_UPDATER_ENDPOINT = "https://github.com/ORG/REPO/releases/download/stable/latest.json"
pnpm updater:manifest
node scripts/release-check.mjs --require-installer --require-updater
```

如果下载、签名或版本校验失败，当前版本会继续运行。回滚使用上一个已签名 Release 重新发布其 `latest.json` 和安装包；不要关闭签名校验，也不要把 HTTP endpoint 写入 Release 配置。



DeepPi 的数据位置：

```text
%APPDATA%\com.deeppi.desktop\deeppi.db
%APPDATA%\com.deeppi.desktop\settings.json
%APPDATA%\com.deeppi.desktop\backups
```

先退出 DeepPi，再复制这些文件用于备份。不要直接编辑数据库。`settings.json` 每次更新前会自动备份；如果设置文件不可解析，保留原文件并从最近的 `backups\settings-*.json` 恢复。

## 获取诊断信息

发布前检查：

```powershell
pnpm release:check
pnpm check
pnpm test
pnpm security:audit
pnpm licenses:check
```

Release 构建：

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --release
pnpm tauri build --bundles nsis
pnpm tauri build --bundles msi
```

长时间稳定性测试默认运行 8 小时；脚本结束时会断言采样到的 DeepPi 进程树没有残留。先用短时冒烟确认环境：

```powershell
pnpm perf:smoke
```

正式长稳测试：

```powershell
pnpm perf:soak
```

## 目标设备验收清单

以下项目必须在真实 Windows + Pi + WebView2 环境执行，并把截图、版本号和日志路径留在发布记录中：

1. 新建 Pi 任务，输入中文、宽字符、emoji 和组合字符，例如 `测试abc😀é`；确认候选框、光标位置和回显正确。
2. 选中多行文本执行 Ctrl+Shift+C/V，粘贴 Windows 文件路径；确认剪贴板内容和终端布局不被破坏。
3. 使用现有 Pi 扩展 fixture 分别触发 `ctx.ui.custom()`、Overlay、自定义 Editor、外部编辑器和 SIXEL/iTerm 图片输出。
4. 切换任务标签、双列和四宫格，确认隐藏终端恢复后内容、焦点和 resize 正常。
5. 启动 `pnpm perf:soak`，记录完整 8 小时结果、冷启动时间、空闲 CPU/内存和退出后的进程检查。
6. 在正式签名安装包上执行安装、跨版本升级、自更新、降级回滚和失败恢复；不要使用本机临时 updater key 作为发布验收证据。

## 发布限制

正式发布还需要代码签名证书、Tauri updater signing secrets、可访问的 HTTPS 更新源、WebView2 依赖策略以及目标 Windows 设备上的输入法、终端扩展、安装升级和长稳验证。
