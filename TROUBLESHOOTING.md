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

DeepPi 只使用自己托管的运行时（Node、Pi、DSH），不读取电脑上安装的 Pi/Node/npm。托管运行时位于 `%LOCALAPPDATA%\com.deeppi.desktop\runtimes\<组件>\active.json`，实际版本在同级 `versions` 目录；没有指针文件的旧安装继续使用 `current`。缺失或损坏时，请到“设置 → 运行时与更新”安装或修复对应组件（Node 是安装 Pi/DSH 的前提，由应用从官方发行包下载并校对 SHA-256）。指针损坏或指向缺失的运行时会直接报错，不会静默使用本机环境。

确认以下目录可写，并检查日志：

```text
%APPDATA%\com.deeppi.desktop\agents\pi
%LOCALAPPDATA%\com.deeppi.desktop\runtimes\node
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

### DSH 版本固定

DeepPi v1.0 只安装并验证 **DSH 0.1.1-rc.2**。DSH 0.1.5-rc.1 起启动 URL 会携带进程 token，且 Host API 需要浏览器交互签发的 cookie，宿主的会话同步与就绪探测尚未适配；因此检查更新时若上游版本更高，应用只显示提示，不提供安装；即使绕过界面直接请求，安装接口也会拒绝非验证版本。

如果因早期版本误装了其他 DSH，运行时页面会把该组件标为“可更新 · 0.1.1-rc.2”，点“更新”即可回到已验证版本。

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

## 内容搜索提示「未找到 ripgrep」

文件侧栏的**内容**搜索依赖系统 `PATH` 上的 `rg.exe`（ripgrep）。DeepPi 不自带、也不会自动下载它：

- 提示「未找到 ripgrep (rg.exe)，请配置系统 PATH 后重试」时，请自行安装 ripgrep 并确保 `rg.exe` 所在目录在 `PATH` 中，然后重启应用。
- 只安装后不重启不会生效：应用在启动时解析 `PATH`。
- 文件名搜索（默认的「文件」模式）不依赖 ripgrep，未安装时仍可正常使用。

## 获取诊断信息

### 文件列表或搜索结果未更新

当前选中项目使用原生目录监听，并每 30 秒触发一次有界核对。连续变更会合并，正在进行的搜索/读取完成后才补刷新；这不是逐事件实时显示保证。文件树顶部仍可手动刷新。

目录被替换、访问失败或应用挂起后租约过期时，文件侧栏会显示监听状态及重新连接按钮。监听只订阅选中项目根；外置 Git 元数据依靠定期核对、返回应用及手动刷新。

外部文件变化不会自动保存或放弃内置草稿。出现磁盘版本冲突时，先比较，再决定重载或另存。构建目录的高频变化不重建文件索引，但仍可能触发 Git 状态核对。

### Pi RPC 报告

桌面应用的“设置 → 高级与诊断 → Pi RPC 诊断”可刷新事件、按运行批次筛选、清空记录及导出当前快照。

- 当前报告包含应用版本、系统/架构、相对启动时间、固定 RPC 事件码、计数和退出码。只保留本次应用运行的最近 256 条记录，相邻同类事件会合并计数；任务进程退出后记录仍可读取，应用退出后不保留。
- `spawn_not_found` 表示找不到程序或工作目录，`spawn_denied` 表示启动被拒绝；`process_ownership_failed` / `process_resume_failed` 分别表示进程树托管或恢复失败，其他创建错误归入 `spawn_failed`。
- `invalid_json` / `incomplete_frame` / `frame_too_large` 区分协议格式、末帧截断和大小超限；`history_failed` 表示历史解析或临时存储失败，其他输出错误归入 `output_invalid`。`request_timeout` 表示请求未按时返回；`response_failed` 表示 Pi 命令返回失败；`stop_timeout` 表示进程收尾未在等待预算内结束。报告不提供任意错误原文。
- `stderr_observed` 的计数单位为字节，其他事件为次数或缺失事件数量。stderr 原文可能包含提示词或凭据，因此不保存、展示或导出；不能用该字节数判断错误的具体内容。
- 报告不收集项目/任务名、文件内容、路径、模型地址、原生会话 ID 或凭据。不打包已有应用日志、配置文件或数据库，不自动上传。
- 导出使用当前预览的快照，预览 10 分钟后失效，刷新可重新生成。原生保存对话框只创建新文件，不覆盖已有文件；取消不会创建报告。写入失败时可能留下不完整报告，先核对目标再选择新文件名，不自动重试。
- 清空仅影响诊断记录和未导出的预览，不删除会话、不停止任务，也不删除已经导出的报告。运行中的任务仍会继续产生新记录。

此入口目前仅覆盖 Pi RPC 传输与进程事件，不替代 DSH、终端兼容模式、运行时安装日志或崩溃诊断。提交报告前仍请核对内容，避免附上原始凭据、提示词或私有源码。

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

## 本地跑 Rust 全量测试偶发 Git 用例失败

在装有安全软件钩子（例如注入 `git.exe` 的 SkyGuard / `sgephook_git.dll`）的机器上，`cargo test` 默认按 CPU 核数并发跑数百个用例时会大量并发启动 `git.exe`，偶发被注入层打断（`git` 以异常码退出、stderr 为空），表现为某个 `git_*` 用例报「无法解析当前提交」或夹具 `git add` 失败。这是环境问题，不是产品缺陷：

- 单独重跑该用例即可通过；CI（无该注入）持续全绿。
- 本地建议用有界并发跑全量：`cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=6`；需要跑联网/长用例时再加 `--include-ignored`。

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

### 本地构建 MSI 时无法下载 WiX

Tauri 打包 MSI 需要 WiX 3.14。首次构建会从 `github.com/wixtoolset` 下载；网络受限时会卡在下载阶段。可手动准备：

```powershell
curl.exe -sL -o "$env:TEMP\wix.nupkg" https://api.nuget.org/v3-flatcontainer/wix/3.14.1/wix.3.14.1.nupkg
expand-archive "$env:TEMP\wix.nupkg" "$env:TEMP\wix"
New-Item -ItemType Directory -Force "$env:LOCALAPPDATA\tauri\WixTools314" | Out-Null
Copy-Item "$env:TEMP\wix\tools\*" "$env:LOCALAPPDATA\tauri\WixTools314" -Recurse -Force
pnpm tauri build --bundles msi
```

GitHub runner 直连正常，CI 与 Release workflow 无需该步骤。
