/** 英文目录（应用外壳）。键 = 简体中文原文。 */
export const EN_SHELL: Record<string, string> = {
  /* ---------- 工作区外壳：控制柱、顶栏、加载状态 ---------- */
  "工作区": "Workspace",
  "工作区启动": "Workspace startup",
  "工作区导航": "Workspace navigation",
  "从工作区移除": "Remove from workspace",
  "Pi 工作区": "Pi workspace",
  "DSH 工作区": "DSH workspace",
  "任务看板": "Task board",
  "任务看板：总览所有任务状态": "Task board: overview of all task statuses",
  "关闭任务看板": "Close task board",
  "任务看板窗口打开失败：{error}": "Failed to open the task board window: {error}",
  "设置": "Settings",
  "正在加载工作区…": "Loading workspace…",
  "重新加载": "Reload",
  "正在加载终端…": "Loading terminal…",
  "终端组件加载失败": "Failed to load the terminal component",
  "重新加载终端组件": "Reload terminal component",
  "编辑器加载失败：{error}": "Failed to load the editor: {error}",
  "恢复副本界面加载失败：{error}": "Failed to load the recovery copies panel: {error}",
  "未选择任务": "No task selected",
  "尚未选择项目": "No project selected",
  "未选择项目": "No project selected",

  /* ---------- 顶栏工具与面板可见性 ---------- */
  "切换 Git 变更栏": "Toggle Git changes panel",
  "隐藏 Git 变更栏": "Hide Git changes panel",
  "显示 Git 变更栏": "Show Git changes panel",
  "单任务": "Single task",
  "双列": "Two columns",
  "网格": "Grid",
  "项目侧栏": "Project sidebar",
  "拖拽调整项目侧栏宽度": "Drag to resize the project sidebar",
  "拖拽调整宽度": "Drag to resize",
  "隐藏项目侧栏": "Hide project sidebar",
  "展开项目侧栏": "Show project sidebar",
  "文件侧栏": "File sidebar",
  "拖拽调整文件栏宽度": "Drag to resize the file panel",
  "隐藏文件栏": "Hide file panel",
  "展开文件栏": "Show file panel",
  "文件所属项目": "Project for this file",
  "恢复副本": "Recovery copies",
  "恢复副本…": "Recovery copies…",

  /* ---------- DSH ---------- */
  "正在启动 DSH": "Starting DSH",
  "DSH 启动失败": "DSH failed to start",
  "DSH 启动失败：{error}": "DSH failed to start: {error}",
  "重启 DSH": "Restart DSH",
  "修复": "Repair",
  "提示：某个 DSH 插件与当前 DSH 版本不兼容。可以到 设置 → DSH 服务 点“修复”处理。":
    "Hint: a DSH plugin is incompatible with the current DSH version. Go to Settings → DSH Service and click “Repair”.",
  "打开 设置 → DSH 服务，查看失败原因并修复":
    "Open Settings → DSH Service to inspect the failure and repair it",
  "DSH 检测/修复操作正在处理，请先完成或取消操作":
    "A DSH check/repair operation is in progress. Finish or cancel it first.",
  "没有可安装的 DSH 运行时版本，请在“运行时与更新”里检查更新。":
    "No installable DSH runtime version. Check for updates in “Runtime & Updates”.",

  /* ---------- 对话框、确认与错误提示 ---------- */
  "操作失败": "Operation failed",
  "知道了": "Got it",
  "确认": "Confirm",
  "保存": "Save",
  "关闭": "Close",
  "更新": "Update",
  "回滚": "Roll back",
  "移除": "Remove",
  "重启": "Restart",
  "未保存的文件": "Unsaved files",
  "保存后继续": "Save and continue",
  "放弃未保存的修改": "Discard unsaved changes",
  "关闭 DeepPi": "Close DeepPi",
  "退出 DeepPi": "Quit DeepPi",
  "退出应用": "Quit app",
  "最小化": "Minimize",
  "选择点击窗口关闭按钮时的默认行为。这个选择会记住，也可以在设置中修改。":
    "Choose the default behavior when the window close button is clicked. This choice is remembered and can be changed in Settings.",
  "仍有活动任务，停止任务并退出吗？": "There are still active tasks. Stop them and quit?",
  "输入任务名称": "Enter task name",
  "选择 Pi 项目目录": "Select Pi project folder",
  "回滚组件": "Roll back component",
  "更新组件": "Update component",
  "恢复 {name} 的上一版本吗？": "Restore {name} to the previous version?",
  "下载并激活 {name} {version} 吗？当前运行时会保留。":
    "Download and activate {name} {version}? The current runtime will be kept.",
  "当前操作尚未接受取消，或已进入提交阶段。":
    "The current operation has not accepted cancellation yet, or has already entered the commit phase.",
  "{id} 未安装": "{id} is not installed",
  "重启 Pi 任务": "Restart Pi tasks",
  "重启 {count} 个运行中的 Pi 任务吗？会话内容保留，正在切换中的任务将被跳过。":
    "Restart {count} running Pi tasks? Session content is kept, and tasks that are switching modes will be skipped.",
  "诊断操作正在处理，请完成或取消后再关闭窗口":
    "A diagnostics operation is in progress. Finish or cancel it before closing the window.",
  "诊断操作正在处理，请先完成或取消操作":
    "A diagnostics operation is in progress. Finish or cancel it first.",
  "项目文件正在处理，请完成后再关闭窗口":
    "Project files are being processed. Finish that work before closing the window.",
  "组件操作正在处理，请完成或取消后再关闭窗口":
    "A component operation is in progress. Finish or cancel it before closing the window.",
  "Git 写入正在处理，请完成后再关闭窗口":
    "A Git write is in progress. Finish it before closing the window.",
  "任务正在切换模式，请完成后再关闭窗口":
    "A task is switching modes. Finish it before closing the window.",
  "清空诊断记录": "Clear diagnostics",
  "清空本次应用运行的 Pi RPC 诊断记录？不会删除任务和会话。":
    "Clear the Pi RPC diagnostics from this app run? Tasks and sessions will not be deleted.",
  "扩展操作正在处理，请先完成或取消操作":
    "An extension operation is in progress. Finish or cancel it first.",

  /* ---------- 项目与任务操作 ---------- */
  "添加项目目录": "Add project folder",
  "移除项目": "Remove project",
  "从工作区移除“{name}”吗？本机目录、任务和会话不会被删除。":
    "Remove “{name}” from the workspace? Local folders, tasks, and sessions will not be deleted.",
  "恢复副本正在处理，请完成后再移除项目。":
    "A recovery copy is being processed. Finish that before removing the project.",
  "确认并发写入": "Confirm concurrent writes",
  "该项目已有活动任务，继续可能产生文件冲突。仍要创建任务吗？":
    "This project already has an active task; continuing may cause file conflicts. Create the task anyway?",
  "新建任务": "New task",
  "重命名任务": "Rename task",
  "修改任务在项目列表中的显示名称。": "Change the name shown for this task in the project list.",
  "删除任务": "Delete task",
  "永久删除“{title}”的 DeepPi 任务记录吗？": "Permanently delete the DeepPi task record for “{title}”?",
  "该任务没有活动终端，请先重启任务": "This task has no active terminal. Restart the task first.",
  "该任务来自旧版本机 Pi 环境，当前稳定版仅支持 DeepPi 托管任务，本机会话记录已保留。":
    "This task comes from an older native Pi environment. The current stable release only supports DeepPi-managed tasks; the local session record has been kept.",
  "请先添加并选择一个 Pi 项目目录": "Add and select a Pi project folder first.",
  "停止“{title}”的当前进程，并使用同一个 Pi 会话继续吗？当前未完成的响应可能中断。":
    "Stop the current process for “{title}” and continue in the same Pi session? Any unfinished response may be interrupted.",
  "切换到对话模式": "Switch to chat mode",
  "切换到终端兼容模式": "Switch to terminal compatibility mode",

  // —— 文件监听与模式切换（src/lib/project-watch.ts、src/lib/task-mode.ts） ——
  "项目目录已变化，请重新连接文件监听": "The project folder changed; reconnect file watching",
  "文件监听已过期，请重新连接": "File watching expired; reconnect",
  "文件监听已中断，可手动刷新或重新连接": "File watching was interrupted; refresh manually or reconnect",
  "文件监听连接已失效，请重新连接": "The file watching connection is no longer valid; reconnect",
  "无法启用文件监听，可手动刷新或重新连接": "Could not enable file watching; refresh manually or reconnect",
  "请先选择未归档的 Pi 项目任务": "Select an unarchived Pi project task first",
  "任务状态已发生变化，请重新选择切换操作": "The task state changed; choose the switch action again",
  "活动任务缺少运行标识，请刷新任务列表": "The active task is missing its run identifier; refresh the task list",

  /* ---------- 任务侧栏（TaskSidebar） ---------- */
  "搜索任务": "Search tasks",
  "清空搜索": "Clear search",
  "任务搜索结果": "Task search results",
  "{count} 个任务": "{count} tasks",
  "没有匹配的任务": "No matching tasks",
  "进行中": "In progress",
  "项目菜单": "Project menu",
  "在 {name} 中新建 Session": "New Session in {name}",
  "新建 Session": "New Session",
  "Session 菜单": "Session menu",
  "移除 Session": "Remove Session",
  "关闭 Session": "Close Session",
  "归档": "Archive",
  "重命名": "Rename",
  "停止": "Stop",
  "恢复": "Restore",

  /* ---------- 标签栏（TaskTabs） ---------- */
  "Session 标签": "Session tabs",
  "关闭 {title}": "Close {title}",
  "向右分割窗口": "Split right",
  "向下分割窗口": "Split down",
  "单栏显示": "Single pane",

  /* ---------- 看板（TaskBoard） ---------- */
  "等待中": "Waiting",
  "正在执行": "Running",
  "中断 / 报错": "Interrupted / Error",
  "已完成": "Completed",
  "共 {total} 个任务 · 运行中 {running} · 等待 {waiting} · 中断 {interrupted} · 已完成 {completed}":
    "{total} tasks · {running} running · {waiting} waiting · {interrupted} interrupted · {completed} completed",
  "另有 {count} 个已归档任务": "{count} more archived tasks",
  "暂无任务": "No tasks",
  "{title}（{count}）": "{title} ({count})",
  "打开任务：{title}": "Open task: {title}",
  "已运行 {duration}": "Running for {duration}",
  "停止任务：{title}": "Stop task: {title}",
  "重开": "Restart",
  "重开任务：{title}": "Restart task: {title}",
  "归档任务：{title}": "Archive task: {title}",
  "删除": "Delete",
  "删除任务：{title}": "Delete task: {title}",
  "{hours} 小时 {minutes} 分": "{hours} h {minutes} min",
  "{minutes} 分 {seconds} 秒": "{minutes} min {seconds} s",
  "{seconds} 秒": "{seconds} s",
  "刚刚": "Just now",
  "{minutes} 分钟前": "{minutes} min ago",
  "{hours} 小时前": "{hours} h ago",
  "昨天": "Yesterday",
  "{days} 天前": "{days} d ago",

  /* ---------- 任务状态标签（src/lib/task.ts: statusLabels） ---------- */
  "运行中": "Running",
  "等待输入": "Waiting for input",
  "失败": "Failed",
  "已取消": "Cancelled",
};
