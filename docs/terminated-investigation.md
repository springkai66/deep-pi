# terminated / WebSocket 调查与修复记录

日期：2026-09-29（本机 UTC+08:00）。

## 结论与边界

已经复现并修复 DeepPi 的自动重试状态遗漏及中继接受不完整 CONNECT 响应的问题；已补齐模型传输原因与中继结束方向的脱敏持久记录。**尚未确定用户此前那次断流由网络链路中的哪一跳触发，也未完成真实 Codex 订阅成功回复与断线恢复验收。**不能把这些代码修复说成已经消除了所有外部断流。

## 现场证据

- 直接通过 Windows UI Automation 操作运行中的 DeepPi 主窗口，读取「高级与诊断 → Pi RPC 诊断 → 报告 JSON」，未读取聊天正文。
- 该快照 `elapsedMs=1147668`、`droppedEvents=0`，仅有 `process_starting`（run 1，+12127ms）与 `replay_gap`（+867761ms，count 1124）。没有进程退出或宿主读写失败事件。
- 同一 Pi RPC 进程从 13:02:06 至 13:27 仍在运行。此证据不支持“此前这些错误都由 RPC 进程反复退出造成”。13:28 左右旧实例消失，系统应用错误事件没有提供退出原因，不将它反推为原始断流根因。
- `replay_gap.count` 是订阅时已移出事件缓冲区的累计事件数量；多个订阅记录还可能被合并。它不是网络断开次数，也不能直接当成消息丢失次数。
- 旧宿主持久日志只提取到中继启动事件；原实现没有保存 CONNECT 双向 copy 结束原因，也没有模型传输原因。缺少记录不代表网络或中继正常。
- 白名单读取到原生 Pi 环境 `retry.maxRetries=10`，托管环境未覆盖默认值。未确认原故障任务绑定哪个环境，因此只作为可能放大重复报错的配置差异，未修改个人设置。

## 复现与代码缺陷

### 1. 不同传输错误在 Pi 变成同一个 terminated

使用当前托管 Pi 0.85.1 的真实 Codex 处理器与 Undici 8.9.0，全部请求指向本机回环服务：

| 注入故障 | 底层原因 | Pi 最终错误 |
| --- | --- | --- |
| 发出首个 SSE 事件后关闭 socket | `UND_ERR_SOCKET` | `terminated` |
| 发出事件后保持空闲，超过实验的 1200ms 阈值 | `UND_ERR_BODY_TIMEOUT` | `terminated` |

Pi 的 `openai-codex-responses.js` 最终 catch 使用 `normalizeProviderError` / `formatProviderError`，后者只保留顶层 message 等字段，没有保留嵌套的 `cause.code`。SSE 路径没有生成带底层原因的诊断条目。这个缺口使旧现场无法区分超时与 socket 关闭。

处理：新增宿主预加载模块 `src-tauri/resources/pi-transport-observer.mjs`，订阅 Node 诊断事件，在原因丢失之前记录固定协议、阶段、白名单错误码、耗时与可用的空闲时间；WebSocket 记录异常关闭码。模块不替换 fetch 或 dispatcher，不改变超时与重试策略，不重发模型请求。

实际 RPC 测试进一步发现 Pi 的输出保护会把后续 `process.stdout.write` 重定向到 stderr。观测器须在 Pi 启动前保存原始 writer，仅通过它发送白名单诊断帧。该边界已经由完整 bundled Pi RPC 测试覆盖。

Rust `pi_transport.rs` 再次校验字段、枚举和数字范围。`rpc_transport.rs` 消费私有诊断帧，不将它们放入会话回放。`diagnostics.rs` 将安全字段放入内存报告并写入轮转日志（`event=pi_diagnostic`）。不保存 URL、地址、头部、正文、堆栈、任意错误文本或凭据。

### 2. 自动重试被显示为持续的最终失败

Pi 会发送 `auto_retry_start`、`auto_retry_end`；原 DeepPi reducer 没有处理它们。故障注入测试确认：重试开始时仍残留 `terminated`，没有文本 delta 的成功重试也不能清掉错误，最终错误更新可能被忽略。

处理：`src/lib/rpc-state.ts` 跟踪重试状态、次数、等待时间、上次错误；成功清除旧错误，失败保留最终错误，直到 `agent_settled` 才释放忙碌状态。`ChatPane.svelte` 显示重试进度。重试开始时间单独记录，无关的 `queue_update` 等事件不再重置倒计时。没有增加自动重发提示词逻辑。

### 3. 上游 CONNECT 响应未结束却被当成成功

原 `read_head_raw` 在状态行后遇 EOF 时，仍可能返回已解析的 200，即使没有收到头部结束空行。中继随后发出成功响应，再立刻让客户端遇到断开。

处理：头部未完整终止就遇 EOF 一律报错，CONNECT 返回 502；直连路径也改为克隆目标 socket 成功后才宣布隧道建立。新增回环假上游回归测试。这个错误发生在握手阶段，**未证实是原现场响应体 terminated 的原因**。

中继现在记录连接编号、结束方向、EOF/系统错误类型及耗时（`event=proxy_tunnel`），不记录目标地址或传输内容。EOF 本身不等于故障；仍须结合对应模型流是否已经完成解释。

## 验证结果

- 新增重试状态测试在修改前失败；修复后通过。独立倒计时回归也经历失败后通过。
- 完整前端测试：554 项通过；此后新增倒计时用例连同相关 reducer/恢复测试共 35 项通过。
- `pnpm check`：0 errors，0 warnings。
- `pnpm build`：生成完成，静态输出写入 `build`。最终组合命令触及工具 60 秒限时，但日志确认检查与构建均已完成。
- `pnpm test:transport`，设置 `PI_RPC_TEST_CLI` 指向当前托管 bundled CLI 后：8 项通过，无跳过。覆盖真实 Pi RPC 的 SSE socket 断开、SSE 空闲超时、WebSocket 首事件后中断；每例均验证同一 RPC 会话仍能响应、只发出一次模型请求。
- Rust：20 项中继测试、10 项诊断测试、2 项传输模块测试通过；RPC 相关 23 项通过，3 项需要外部条件的测试保持忽略。
- `cargo check --manifest-path src-tauri/Cargo.toml` 通过。
- 本轮未发送真实订阅提示词，未修改已安装 Pi、凭据或个人配置。

常用复验命令：

```powershell
pnpm check
pnpm test:web
pnpm test:transport
cargo test --manifest-path src-tauri/Cargo.toml --lib proxy_relay -- --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --lib diagnostics -- --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --lib pi_transport -- --test-threads=1
```

完整 Pi 用例仅在 `PI_RPC_TEST_CLI` 指向已安装 CLI 时启用；该路径之外的所有测试目录、认证标记和服务端均为隔离夹具。它们不等于 Codex 订阅链路验收。

## 尚未完成的现场判定

原始错误链在旧实现中未被保存，不能事后恢复。新建 RPC 进程会加载观测器；下一次自然发生的故障可以从持久日志区分 `UND_ERR_BODY_TIMEOUT`、`UND_ERR_SOCKET`、WebSocket 异常关闭和宿主进程退出，再与中继方向记录关联。此前订阅额度限制阻止成功回复验收，本轮没有重复探测额度或发送付费请求。

## WebSocket error 后续处理（2026-09-29）

用户要求单独修复 WebSocket error 后，又验证了完整托管 Pi 的成功恢复路径。握手返回 426、或首个流事件后断开，两种情况下原生自动模式都能回退/重试为 SSE 并得到完整答案；经过两级真实 DeepPi 中继也通过。因此没有把原始问题归因于“中继不支持 WebSocket”或“Pi 完全不重试”。

添加 OpenAI 订阅后，在「设置 → 模型设置 → 点击 OpenAI 订阅名称或设置按钮 → Codex 传输方式」提供四档选择；网络代理页、未添加订阅时以及其他供应商详情均不显示此选项。选择自动保存到 DeepPi 配置的 `codexTransport` 字段，旧配置缺少该字段时默认 SSE。RPC 启动时加载宿主扩展 `pi-codex-transport.mjs`，仅控制 `openai-codex` 的 `openai-codex-responses` API：

| 传输选择 | DeepPi RPC 行为 |
| --- | --- |
| SSE（推荐，默认） | 请求开始前直接选择 SSE，不先建立 WebSocket |
| 自动（WebSocket 优先） | 使用 Pi 原生自动模式，按传输阶段回退或重试为 SSE |
| WebSocket | 尝试 WebSocket，不启用缓存续传；保留 Pi 原生回退/重试 |
| WebSocket（缓存续传） | 启用连接复用与增量续传；保留 Pi 原生回退/重试 |

这是默认传输选择的稳定性处理，不是对远端 WebSocket 服务的修复。默认模式不再使用 WebSocket 连接复用/增量续传优化；SSE 仍是流式响应，同样可能遇到网络断开。没有增加提示词重发逻辑、修改超时或关闭错误显示。

扩展通过 Pi 的 provider 注册接口只覆盖流式函数，不替换模型目录、base URL、认证或账户设置。SDK 模块只从当前托管 Pi 的嵌套/顶层依赖目录加载。界面中的选择优先于 Pi 自身的 transport 配置，但不改写 Pi 的个人配置或安装文件；只对新建或重新启动的 RPC 会话生效，运行中的会话保持原方式。界面在选择框旁明确说明此范围。

新增验证：

- `pnpm test:transport` 在指定托管 CLI 后共 19 项通过，无跳过。
- 10 项完整 Pi 对照：2 种 WebSocket 故障 × 原生自动模式及界面中的四个选项。SSE 测试关闭自动重试，断言 0 次 WebSocket、1 次 SSE、0 次重试，仍获得完整回复；其他界面选项即使 Pi 自身设置为 SSE，也会实际尝试 WebSocket。测试验证会话身份、模型身份及 Pi 配置文件内容保持不变。
- 同一套 10 项对照经过“Pi → DeepPi 中继 → 上游中继 → 本地服务”再次通过，并检查两级连接计数确保未绕过代理。Rust 测试名：`real_pi_websocket_recovers_through_two_proxy_hops`（需设置 `PI_RPC_TEST_CLI` 并加 `--include-ignored`）。
- Rust 设置测试 14 项、传输模块测试 4 项通过，覆盖旧配置默认值、四档持久化、非法值拒绝、SDK 定位与启动参数；前端设置相关 20 项测试通过。
- 隔离 Chromium 验证四个选项的切换、说明更新、保存回调与页面切换后保留；1280px / 960px 桌面宽度下无横向溢出。`pnpm check` 为 0 错误、0 警告，`pnpm build` 与 `cargo check` 通过。

仍未新增真实订阅请求，未完成真实订阅链路验收；旧现场的具体断开方依旧不能从缺失的历史原因码恢复。

## 模型目录勾选与详情操作

OpenAI 订阅与自定义 Provider 的模型详情均使用「全部模型 / 已选模型」共用列表：搜索、逐项勾选、全选、清空和右侧详情。点击模型只打开详情；勾选会立即排队自动保存，无需点击「保存选择」。连续操作串行提交不可变快照，仅保留最后的待提交意图；失败时保留当前选择与重试入口。OpenAI 专属的传输设置仍在列表上方。

OpenAI 筛选只写 DeepPi 的 `codexSelectedModels`：`null` 表示全部目录（包括未来模型），`[]` 表示无 Codex 候选；不改 Pi 模型或认证配置，也不切换当前会话模型或重发提示词。成功后的 `selection` 通知只刷新聊天候选，不重载会话。自定义 Provider 的勾选只调用受限的 `save_pi_provider_model_selection`，后端对已有模型沿用已保存参数，仅为新模型写入候选参数；右侧未保存的模型/Provider 字段和 API Key 不会随勾选提交。

两种详情均提供「自动填入」与「测试连通」。自动填入仅读取模型目录参数，仍需显式保存参数，且不会增加订阅权限。「测试连通」只由用户点击触发，可能消耗额度；OpenAI 命令经 Pi 托管 OAuth 与 SDK 发出单次有界、无会话工具的最小请求，不使用第三方 keyring，不返回模型输出；自定义 Provider 则使用其已保存的模型与凭据。

验证：隔离浏览器夹具覆盖两套列表、OpenAI 自动筛选、第三方参数草稿不被成员保存提交、连续勾选的最新意图、失败重试及显式探测；队列单元测试 2 项通过。官方探测的 Rust 离线夹具验证托管 OAuth、无网络注入运行时、不返回生成文本及配置不变。此处的本地探测与恢复测试均不等于真实订阅成功回复或线上断线验收；旧现场缺失历史原因码，仍无法追溯断开方。
