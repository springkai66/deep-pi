/** 英文目录（诊断面板、对话框、消息渲染、终端等杂项）。键 = 简体中文原文。 */
export const EN_MISC: Record<string, string> = {
  // —— 应用对话框 AppDialog.svelte ——
  "关闭对话框": "Close",
  "提示": "Notifications",
  "关闭提示": "Dismiss notification",
  // 注意：「关闭」在本项目有两义——推理强度的 off 档（EN_CHAT 里是 "Off"）
  // 和关闭按钮。为避免同一个中文键被两种含义争用，对话框按钮的原文已改为
  // 「关闭对话框」，因此这里只需要上面这一条。
  "知道了": "Got it",
  "确认": "Confirm",
  "取消": "Cancel",

  // —— 消息体 ChatMessage.svelte ——
  "思考": "Thinking",
  "{name} · 工具参数": "{name} · Tool arguments",
  "会话图片": "Conversation image",
  "图片无法解码": "Image could not be decoded",
  "图片类型或大小不受支持。": "This image type or size is not supported.",
  "未识别的消息内容": "Unrecognized message content",
  "复制消息": "Copy message",
  "复制": "Copy",
  "已复制": "Copied",
  "复制失败：{error}": "Copy failed: {error}",
  "原始文本": "Raw text",

  // —— Markdown 渲染 MessageMarkdown.svelte ——
  "图片：{source}": "Image: {source}",
  "消息表格": "Message table",

  // —— 代码块 MessageCodeBlock.svelte ——
  "代码块": "Code block",
  "代码自动换行": "Wrap long lines",
  "复制代码": "Copy code",

  // —— 终端面板 TerminalPane.svelte ——
  " · 正在切换模式": " · Switching modes",
  "切换到对话模式": "Switch to conversation mode",
  "打开命令终端": "Open command terminal",
  "关闭命令终端": "Close command terminal",
  "Shell 已退出（代码 {code}）。": "Shell exited (code {code}).",


  // —— 诊断面板 DiagnosticsPanel.svelte ——
  "Pi RPC 诊断": "Pi RPC diagnostics",
  "刷新诊断": "Refresh diagnostics",
  "清空诊断记录": "Clear diagnostics log",
  "导出当前快照（新文件）": "Export current snapshot (new file)",
  "诊断仅在桌面应用可用": "Diagnostics are only available in the desktop app",
  "正在处理…": "Working…",
  "应用": "App",
  "系统": "System",
  "采集范围": "Scope",
  "本次应用运行 · Pi RPC · 最多 256 条": "This app run · Pi RPC · up to 256 entries",
  "隐私": "Privacy",
  "不含 stderr 原文、提示词、文件内容、路径与凭据": "No raw stderr, prompts, file contents, paths, or credentials",
  "运行批次": "Run",
  "筛选诊断运行批次": "Filter diagnostics by run",
  "全部": "All",
  "较早的 {count} 条记录已被容量限制移除": "The {count} older records were removed by the capacity limit",
  "暂无诊断事件": "No diagnostic events yet",
  "诊断事件": "Diagnostic events",
  "报告 JSON": "Report JSON",

  // —— 兜底：src/lib/diagnostics.ts ——
  // 源头保持中文常量：白名单 SAFE_DIAGNOSTIC_ERRORS 必须与后端报文逐字比较，不能翻译；
  // 面板里用 t(DIAGNOSTIC_LABELS[...]) / tm(t(state.error)) / t(state.status) 取对应译文，缺条目时自动回落中文。
  "进程启动": "Process starting",
  "进程创建失败": "Process spawn failed",
  "进程退出": "Process exit",
  "请求停止": "Stop requested",
  "停止超时": "Stop timed out",
  "输入管道写入失败": "Input pipe write failed",
  "输出协议或读取失败": "Output protocol or read failed",
  "标准错误输出（字节）": "Standard error output (bytes)",
  "标准错误管道读取失败": "Standard error pipe read failed",
  "请求超时": "Request timed out",
  "命令失败": "Command failed",
  "输入队列已满或关闭": "Input queue full or closed",
  "事件回放缺口": "Event replay gap",
  "找不到进程程序或工作目录": "Process program or working directory not found",
  "进程启动被拒绝": "Process spawn denied",
  "进程树托管失败": "Process tree ownership failed",
  "挂起进程恢复失败": "Suspended process resume failed",
  "输出帧超过大小上限": "Output frame exceeds the size limit",
  "输出不是合法的 JSON 对象": "Output is not a valid JSON object",
  "进程退出时输出帧不完整": "Output frame incomplete when the process exited",
  "输出管道读取失败": "Output pipe read failed",
  "历史响应解析或临时存储失败": "History response parse or temporary storage failed",
  "诊断记录暂不可用": "Diagnostic log unavailable",
  "诊断报告正在导出": "A diagnostic report is already being exported",
  "诊断预览已失效，请刷新后重新导出": "The diagnostic preview has expired; refresh and export again",
  "无法生成诊断报告": "Could not generate the diagnostic report",
  "主窗口已关闭": "The main window has closed",
  "不支持该报告保存位置": "That save location is not supported for reports",
  "诊断导出任务失败": "The diagnostic export task failed",
  "报告需要完整的普通文件路径": "Reports require a full regular file path",
  "报告不支持设备路径": "Device paths are not supported for reports",
  "报告文件名无效": "Invalid report file name",
  "无法创建报告，请选择可写目录和新的文件名；不会覆盖已有文件": "Could not create the report; choose a writable directory and a new file name. Existing files are not overwritten",
  "报告写入未完成，目标可能保留不完整文件，请核对后另选文件名": "The report write did not finish; the target may hold an incomplete file. Check it and choose another file name",
  "无法读取诊断，请重试": "Could not read the diagnostics; try again",
  "报告已导出": "Report exported",
  "已取消导出": "Export cancelled",
  "无法确认报告是否导出；请检查保存位置。预览过期时请刷新后重试": "Could not confirm whether the report was exported; check the save location. If the preview expired, refresh and try again",
  "诊断记录已清空": "Diagnostics log cleared",
  "清空或刷新未完成，请重新读取诊断记录": "Clearing or refreshing did not finish; reload the diagnostics log",

  // —— 消息渲染：src/lib/message-parts.ts、src/lib/rpc-state.ts ——
  "思考内容不可用": "Thinking content unavailable",
  "工具": "Tool",
  "[图片 {mime}]": "[Image {mime}]",
  "[图片]": "[Image]",
  "RPC 运行失败": "RPC failed",

  // —— 终端输入：src/lib/terminal-input.ts（TerminalPane.svelte 显示） ——
  "终端输入队列已满，请等待当前输入完成": "The terminal input queue is full; wait for the current input to finish",

  // —— 兜底：FilePreview.svelte ——
  // 该文件由文件面板 agent 负责接线，本次未改动；先把词条放进来，
  // 一旦它接上 t()（或别的目录文件给出同名键）即可生效。
  "已发送到外部编辑器": "Sent to the external editor",
  "文件预览": "File preview",
  "只读": "Read-only",
  "在外部编辑器中打开": "Open in external editor",
  "配置外部编辑器": "Configure external editor",
  "重新读取": "Reload",
  "重新读取文件": "Reload file",
  "关闭预览": "Close preview",
  "关闭文件预览": "Close file preview",
  "正在读取文件…": "Reading file…",
  "字节": "bytes",
};
