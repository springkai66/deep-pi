/** 后端消息分片：应用生命周期、设置、扩展与其余模块。 */
export const APP_MESSAGES_CORE: Record<string, import("./app-messages").AppMessageText> = {
  "app.startup.not_ready": {
    "zh-CN": "应用正在初始化，请稍后重试",
    "zh-TW": "應用正在初始化，請稍後重試",
    en: "The app is still starting up. Try again in a moment.",
  },
  // —— 扩展与任务（从 Rust 模块迁移） ——
  "package.tasks_running": {
    "zh-CN": "请先停止所有 Pi 任务，再修改扩展，以确保可以安全恢复",
    "zh-TW": "請先停止所有 Pi 任務，再修改擴展，以確保可以安全恢復",
    en: "Stop all Pi tasks before changing extensions so they can be restored safely.",
  },
  "pty.terminal_busy": {
    "zh-CN": "终端仍在结束输出，请稍后重试切换",
    "zh-TW": "終端仍在結束輸出，請稍後重試切換",
    en: "The terminal is still finishing its output. Try switching again in a moment.",
  },
  "rpc.history.cursor_invalid": {
    "zh-CN": "历史分页位置无效",
    "zh-TW": "歷史分頁位置無效",
    en: "The history pagination cursor is invalid.",
  },
  "rpc.history.expired": {
    "zh-CN": "历史快照已失效或空闲过期，请重新打开任务",
    "zh-TW": "歷史快照已失效或空閒過期，請重新打開任務",
    en: "The history snapshot is no longer valid or has expired from inactivity. Reopen the task.",
  },
  "rpc.history.lock_unavailable": {
    "zh-CN": "历史快照锁不可用",
    "zh-TW": "歷史快照鎖不可用",
    en: "The history snapshot lock is unavailable.",
  },
  "rpc.history.message_invalid": {
    "zh-CN": "历史消息格式无效",
    "zh-TW": "歷史消息格式無效",
    en: "The history message format is invalid.",
  },
  "rpc.history.message_serialize_failed": {
    "zh-CN": "无法序列化历史消息",
    "zh-TW": "無法序列化歷史消息",
    en: "Failed to serialize the history message.",
  },
  "rpc.history.read_in_progress": {
    "zh-CN": "历史快照正在读取，请稍后重试",
    "zh-TW": "歷史快照正在讀取，請稍後重試",
    en: "The history snapshot is being read. Try again in a moment.",
  },
  "rpc.history.response_invalid": {
    "zh-CN": "历史响应格式无效或单条消息超过上限",
    "zh-TW": "歷史響應格式無效或單條消息超過上限",
    en: "The history response is malformed or a single message exceeds the size limit.",
  },
  "rpc.history.response_missing": {
    "zh-CN": "历史快照响应缺失",
    "zh-TW": "歷史快照響應缺失",
    en: "The history snapshot response is missing.",
  },
  "rpc.history.response_trailing_content": {
    "zh-CN": "历史响应存在多余内容",
    "zh-TW": "歷史響應存在多餘內容",
    en: "The history response contains trailing content.",
  },
  "rpc.history.size_limit_reached": {
    "zh-CN": "历史快照占用已达上限，请关闭不用的任务后重试",
    "zh-TW": "歷史快照占用已達上限，請關閉不用的任務後重試",
    en: "The history snapshot size limit has been reached. Close unused tasks and try again.",
  },
  "rpc.history.snapshot_invalid": {
    "zh-CN": "历史快照内容无效",
    "zh-TW": "歷史快照內容無效",
    en: "The history snapshot contents are invalid.",
  },
  "rpc.history.snapshot_read_failed": {
    "zh-CN": "无法读取历史快照",
    "zh-TW": "無法讀取歷史快照",
    en: "Failed to read the history snapshot.",
  },
  "rpc.history.snapshot_seek_failed": {
    "zh-CN": "无法定位历史快照",
    "zh-TW": "無法定位歷史快照",
    en: "Failed to seek in the history snapshot.",
  },
  "rpc.history.spool_failed": {
    "zh-CN": "无法创建临时历史快照",
    "zh-TW": "無法創建臨時歷史快照",
    en: "Failed to create the temporary history snapshot.",
  },
  "rpc.history.spool_write_failed": {
    "zh-CN": "无法写入临时历史快照",
    "zh-TW": "無法寫入臨時歷史快照",
    en: "Failed to write the temporary history snapshot.",
  },
  "rpc.outcome_unknown": {
    "zh-CN": "RPC_OUTCOME_UNKNOWN: 未收到请求确认，操作可能已经执行。请重新连接并检查会话，勿直接重复发送。({error})",
    "zh-TW": "RPC_OUTCOME_UNKNOWN: 未收到請求確認，操作可能已經執行。請重新連接並檢查會話，勿直接重復發送。({error})",
    en: "RPC_OUTCOME_UNKNOWN: The request was not acknowledged, so the operation may have already run. Reconnect and check the session instead of sending it again. ({error})",
  },
  "task.already_running": {
    "zh-CN": "任务已在运行",
    "zh-TW": "任務已在運行",
    en: "The task is already running.",
  },
  "task.archived_restore_first": {
    "zh-CN": "归档的任务需要先恢复才能重启",
    "zh-TW": "歸檔的任務需要先恢復才能重啟",
    en: "Restore the archived task before restarting it.",
  },
  "task.bridge_status_invalid": {
    "zh-CN": "桥接状态必须是运行中或等待输入",
    "zh-TW": "橋接狀態必須是運行中或等待輸入",
    en: "The bridge status must be running or waiting for input.",
  },
  "task.finish_status_invalid": {
    "zh-CN": "结束的任务状态必须是终态",
    "zh-TW": "結束的任務狀態必須是終態",
    en: "A finished task must have a terminal status.",
  },
  "task.not_found": {
    "zh-CN": "未找到该任务",
    "zh-TW": "未找到該任務",
    en: "The task was not found.",
  },
  "task.project_default_name": {
    "zh-CN": "项目",
    "zh-TW": "項目",
    en: "Project",
  },
  "task.project_missing": {
    "zh-CN": "项目目录不存在",
    "zh-TW": "項目目錄不存在",
    en: "The project directory does not exist.",
  },
  "task.session_id_invalid": {
    "zh-CN": "任务的 Pi 会话 ID 无效",
    "zh-TW": "任務的 Pi 會話 ID 無效",
    en: "The task's Pi session ID is invalid.",
  },
  "task.stop_before_archive": {
    "zh-CN": "请先停止正在运行的任务，再归档",
    "zh-TW": "請先停止正在運行的任務，再歸檔",
    en: "Stop the running task before archiving it.",
  },
  "task.stop_before_delete": {
    "zh-CN": "请先停止正在运行的 Pi 任务，再删除",
    "zh-TW": "請先停止正在運行的 Pi 任務，再刪除",
    en: "Stop the running Pi task before deleting it.",
  },
  "task.title_length": {
    "zh-CN": "任务标题长度必须在 1 到 200 个字符之间",
    "zh-TW": "任務標題長度必須在 1 到 200 個字符之間",
    en: "The task title must be between 1 and 200 characters long.",
  },

  // —— 主题文件与应用生命周期（新文件，原文手工回填） ——
  "app.main_window_closed": {
    "zh-CN": "主窗口已关闭",
    "zh-TW": "主視窗已關閉",
    en: "The main window is closed.",
  },
  "theme.main_window_only": {
    "zh-CN": "主题设置仅允许主窗口访问",
    "zh-TW": "主題設定僅允許主視窗存取",
    en: "Theme settings are only available from the main window.",
  },
  "theme.path_required": {
    "zh-CN": "主题文件需要完整的普通文件路径",
    "zh-TW": "主題檔案需要完整的普通檔案路徑",
    en: "A theme file needs a complete regular file path.",
  },
  "theme.path_local_disk_only": {
    "zh-CN": "主题文件需要位于本地磁盘",
    "zh-TW": "主題檔案需要位於本機磁碟",
    en: "A theme file must live on a local disk.",
  },
  "theme.path_parent_dir": {
    "zh-CN": "主题文件路径不能包含上级目录",
    "zh-TW": "主題檔案路徑不能包含上層目錄",
    en: "A theme file path cannot contain a parent directory.",
  },
  "theme.export_not_object": {
    "zh-CN": "主题内容必须是一个 JSON 对象",
    "zh-TW": "主題內容必須是一個 JSON 物件",
    en: "The theme content must be a JSON object.",
  },
  "theme.serialize_failed": {
    "zh-CN": "主题序列化失败：{error}",
    "zh-TW": "主題序列化失敗：{error}",
    en: "Failed to serialize the theme: {error}",
  },
  "theme.export_too_large": {
    "zh-CN": "主题内容过大，已拒绝导出",
    "zh-TW": "主題內容過大，已拒絕匯出",
    en: "The theme is too large to export.",
  },
  "theme.export_dialog_title": {
    "zh-CN": "导出主题文件",
    "zh-TW": "匯出主題檔案",
    en: "Export theme file",
  },
  "theme.file_filter": {
    "zh-CN": "DeepPi 主题",
    "zh-TW": "DeepPi 主題",
    en: "DeepPi theme",
  },
  "theme.export_location_unsupported": {
    "zh-CN": "不支持该主题保存位置",
    "zh-TW": "不支援該主題儲存位置",
    en: "That save location is not supported for themes.",
  },
  "theme.write_failed": {
    "zh-CN": "主题写入失败：{error}",
    "zh-TW": "主題寫入失敗：{error}",
    en: "Failed to write the theme: {error}",
  },
  "theme.export_failed": {
    "zh-CN": "主题导出失败：{error}",
    "zh-TW": "主題匯出失敗：{error}",
    en: "Theme export failed: {error}",
  },
  "theme.import_dialog_title": {
    "zh-CN": "导入主题文件",
    "zh-TW": "匯入主題檔案",
    en: "Import theme file",
  },
  "theme.import_location_unsupported": {
    "zh-CN": "不支持该主题文件位置",
    "zh-TW": "不支援該主題檔案位置",
    en: "That theme file location is not supported.",
  },
  "theme.file_unreadable": {
    "zh-CN": "主题文件不可读：{error}",
    "zh-TW": "主題檔案無法讀取：{error}",
    en: "The theme file cannot be read: {error}",
  },
  "theme.path_not_a_file": {
    "zh-CN": "主题路径不是一个文件",
    "zh-TW": "主題路徑不是一個檔案",
    en: "The theme path is not a file.",
  },
  "theme.file_too_large": {
    "zh-CN": "主题文件过大（上限 256 KB）",
    "zh-TW": "主題檔案過大（上限 256 KB）",
    en: "The theme file is too large (limit 256 KB).",
  },
  "theme.read_failed": {
    "zh-CN": "主题文件读取失败：{error}",
    "zh-TW": "主題檔案讀取失敗：{error}",
    en: "Failed to read the theme file: {error}",
  },
  "theme.import_failed": {
    "zh-CN": "主题导入失败：{error}",
    "zh-TW": "主題匯入失敗：{error}",
    en: "Theme import failed: {error}",
  },
  "market.mcp_query_invalid": {
    "zh-CN": "搜索关键词无效：请控制在 100 个字符以内，且不含控制字符",
    "zh-TW": "搜尋關鍵字無效：請控制在 100 個字元以內，且不含控制字元",
    en: "Invalid search query: keep it under 100 characters and free of control characters.",
  },
  "market.agenticskills_slug_invalid": {
    "zh-CN": "市场条目标识无效",
    "zh-TW": "市場條目標識無效",
    en: "The marketplace entry identifier is invalid.",
  },
  "market.agenticskills_request_failed": {
    "zh-CN": "无法访问 agenticskills.io：{error}",
    "zh-TW": "無法存取 agenticskills.io：{error}",
    en: "agenticskills.io could not be reached: {error}",
  },
  "market.agenticskills_detail_missing": {
    "zh-CN": "agenticskills.io 返回的页面结构无法识别，站点可能已改版",
    "zh-TW": "agenticskills.io 回應的頁面結構無法識別，站點可能已改版",
    en: "The page returned by agenticskills.io could not be recognised; the site may have changed.",
  },
  "market.agentic_skill_no_source": {
    "zh-CN": "该技能没有可直接下载的 SKILL.md 来源，请在站点上手动安装",
    "zh-TW": "該技能沒有可直接下載的 SKILL.md 來源，請在站點上手動安裝",
    en: "This Skill has no directly downloadable SKILL.md source; install it manually on the site.",
  },
  "market.agentic_mcp_no_config": {
    "zh-CN": "该服务器没有可自动写入的配置片段，请在站点上按文档手动配置",
    "zh-TW": "該伺服器沒有可自動寫入的設定片段，請在站點上依文件手動設定",
    en: "This server has no configuration snippet that can be written automatically; configure it manually following the site's documentation.",
  },
  "market.agentic_mcp_config_unparsed": {
    "zh-CN": "站点给出的配置片段无法解析为 JSON，请手动配置",
    "zh-TW": "站點給出的設定片段無法解析為 JSON，請手動設定",
    en: "The configuration snippet from the site could not be parsed as JSON; configure it manually.",
  },
  // —— 本机 Pi 元数据与应用启动失败 ——
  "app.init_thread_failed": {
    "zh-CN": "应用初始化工作线程失败",
    "zh-TW": "應用初始化工作執行緒失敗",
    en: "The app initialization worker thread failed.",
  },
  "app.install_dir_unknown": {
    "zh-CN": "无法确定 DeepPi 安装目录",
    "zh-TW": "無法確定 DeepPi 安裝目錄",
    en: "The DeepPi installation directory could not be determined.",
  },
  "native_pi.session_metadata_unreadable": {
    "zh-CN": "无法检查会话元数据",
    "zh-TW": "無法檢查工作階段中繼資料",
    en: "The session metadata could not be checked.",
  },
};
