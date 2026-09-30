/** 英文目录（对话框、消息渲染、终端等杂项）。键 = 简体中文原文。 */
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
  // —— 桌宠与任务通知 ——
  "桌宠": "Desktop pet",
  "显示桌宠": "Show the desktop pet",
  "关闭桌宠": "Close the pet",
  "桌宠窗口打开失败：{error}": "Failed to open the pet window: {error}",
  "任务已完成": "Task completed",
  "任务失败": "Task failed",
  "任务完成！": "All done! 🎉",
  "任务失败…": "The task failed…",
  "嗨，我在呢～": "Hi, I'm here~",
  "有任务尽管交给我！": "Just hand me a task!",
  "咯咯——": "Beep boop—",
  "一起加油鸭！": "Let's go!",
  "现在没有执行中的任务": "No running tasks right now",
  "桌宠，悬停查看任务": "Desktop pet; hover to view tasks",
  "桌宠任务": "Pet tasks",
  "桌宠已关闭": "The pet was closed",
  "正在读取任务…": "Loading tasks…",
  "暂无进行中任务": "No tasks in progress",
  "未命名任务": "Untitled task",
  "还有 {count} 个任务": "{count} more tasks",
  "在主窗口打开：{title}": "Open in the main window: {title}",
  "任务环半径": "Task ring radius",
  "气泡环绕桌宠的距离；调近时气泡会自动收窄，任务多到放不下才会略微外扩。":
    "How far the bubbles orbit the desktop pet. Pull it in and the bubbles narrow automatically; only a very full ring grows outward.",
  "结束任务：{title}": "End task: {title}",
  "停止当前执行，保留会话": "Stop the current run; keep the session",
  "DSH 任务请在主窗口操作": "Manage DSH tasks from the main window",
  "任务正在处理，请稍后重试": "The task is busy; try again shortly",
  "任务状态已改变，请刷新后重试": "The task changed; refresh and try again",
  "任务操作结果未确认，请刷新核对后再试": "The task action result was not confirmed; refresh and verify before retrying",
  "任务操作失败，请重试": "The task action failed; try again",
  "无法读取任务，请重试": "Could not load tasks; try again",
  "桌宠窗口调整失败，请重试": "Could not resize the pet window; try again",
  "主窗口正在处理设置操作，请稍后继续": "The main window is finishing a settings operation; try again shortly",
  "任务控制尚未就绪，请重试": "Task controls are not ready yet; try again",
};
