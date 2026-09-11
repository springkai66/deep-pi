# Pi RPC 兼容基线

本文件锁定 DeepPi「Pi RPC 工作区」已验证的 Pi 版本、协议子集与宿主行为。终端兼容模式（TUI）不受本表限制。

- 基线版本：Pi 0.84.4（DeepPi 托管运行时，`@earendil-works/pi-coding-agent`）
- 官方文档：随包分发 `docs/rpc.md`（Pi 0.84.4），本文只声明该版本上验证过的子集
- DSH 基线：0.1.1-rc.2（dshmarket 最低要求），DSH 使用原生 Web UI，不经过本协议

## 传输与分帧

- stdin/stdout JSONL；仅以 LF (`\n`) 分帧，容忍行尾 `\r`；不把 Unicode 分隔符当换行。
- 命令可带 `id`；响应为 `{"type":"response", ...}` 并回带同一 `id`；事件不带 `id`。
- 普通帧上限 8 MiB；大历史响应流式落入匿名临时快照，原始整帧上限 128 MiB。
- 订阅按 `taskId + runId` 绑定，进程退出、断开或取消时释放；迟到响应按请求 ID 丢弃。
- stderr 独立排空，只累计字节数，不保留原文。

## 已锁定命令

宿主只转发下列命令，其余一律拒绝（“RPC command is not supported by this host”）：

| 命令 | 用途 | 宿主备注 |
| --- | --- | --- |
| `prompt` / `steer` / `follow_up` | 发送、插话、排队 | 校验 1..524288 字节 |
| `get_state` | 会话与模型状态 | 空闲、订阅建立后读取 |
| `get_messages` | 读取原生历史 | 快照分页，不落第二份消息库 |
| `get_available_models` | 模型列表 | 仅用于选择器 |
| `get_commands` | 原生命令列表 | 只读 |
| `set_model` | 切换模型 | 校验后转发 |
| `set_thinking_level` | 推理等级 | 校验后转发 |
| `clear_queue` / `abort` | 停止 | 宿主先清队列再中止 |
| `extension_ui_response` | 扩展交互应答 | 仅应答已收到的请求 |
| `set_session_name` | 会话重命名 | 与任务标题分离 |

## 已处理事件

| 事件 | 宿主行为 |
| --- | --- |
| `agent_start` | 标记忙碌 |
| `agent_settled` | 标记空闲并重新读取 `get_state` |
| `message_start` / `message_update` / `message_end` | 结构化消息；流式更新按角色与时间戳合并 |
| `tool_execution_start` / `tool_execution_update` / `tool_execution_end` | 工具条目；更新使用 `partialResult` |
| `queue_update` | 显示 steering / followUp 队列 |
| `extension_error` | 显示错误 |
| `rpc_exit` / `rpc_error`（宿主合成） | 断开连接、结束运行中的工具条目 |

接收但不映射到界面状态：`agent_end`、`turn_start` / `turn_end`、`bash_execution_update`、`compaction_start` / `compaction_end`、`auto_retry_start` / `auto_retry_end`、`summarization_retry_*`。这些事件要么由消息事件覆盖，要么属于原生 TUI 能力，不作为 RPC 工作区承诺。

## 扩展 UI 协议

- 支持 `confirm`、`select`、`input`、`editor`，路由到宿主模态对话框；取消会回传 `cancelled`。
- 支持宿主侧展示类方法：`notify`、`setStatus`、`setWidget`、`setTitle`、`set_editor_text`。
- 未知方法：取消该请求并提示“此扩展交互需要终端兼容模式”，不静默忽略。

## 原生会话

- 恢复：`--session <file>`，读取前校验会话 ID、项目目录与文件身份；新会话使用 `--session-id`；标题使用 `--name`。
- 历史读取只经 `get_messages` 快照；不复制或改写原生会话文件。
- v1.0 只启动托管环境（`PI_CODING_AGENT_DIR` 指向任务绑定的托管目录）。

## 覆盖测试

- `rpc_transport`：分帧、中文、大消息、非法 JSON、迟到响应、超时、退出与订阅释放。
- `rpc_state` / `rpc_history`：事件合并、分页游标、过期事件与快照身份拒绝。
- `dialog` / `ChatPane`：扩展 UI 确认、选择、输入与取消生命周期。
- 真实 Pi 集成（`cargo test -- --include-ignored`）：发送、工具调用、停止、恢复与历史分页。
