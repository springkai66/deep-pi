/** 后端消息分片：文件、恢复副本与运行时。消息码由 Rust 模块迁移而来，三种语言均已翻译。 */
export const APP_MESSAGES_FILES: Record<string, import("./app-messages").AppMessageText> = {
  // —— 文件、恢复副本、运行时与扩展（从 Rust 模块迁移） ——
  "agent_config.file_too_large": {
    "zh-CN": "Pi 配置文件超过 1 MiB",
    "zh-TW": "Pi 配置文件超過 1 MiB",
    en: "The Pi configuration file exceeds 1 MiB.",
  },
  "agent_config.invalid_json": {
    "zh-CN": "Pi 配置不是有效 JSON: {error}",
    "zh-TW": "Pi 配置不是有效 JSON: {error}",
    en: "The Pi configuration is not valid JSON: {error}",
  },
  "agent_config.mcp_limit_reached": {
    "zh-CN": "MCP 服务数量超过上限",
    "zh-TW": "MCP 服務數量超過上限",
    en: "The number of MCP servers exceeds the limit.",
  },
  "agent_config.mcp_not_object": {
    "zh-CN": "MCP 配置必须是 JSON 对象",
    "zh-TW": "MCP 配置必須是 JSON 對象",
    en: "The MCP configuration must be a JSON object.",
  },
  "agent_config.mcp_too_large": {
    "zh-CN": "MCP 配置过大",
    "zh-TW": "MCP 配置過大",
    en: "The MCP configuration is too large.",
  },
  "agent_config.name_charset": {
    "zh-CN": "名称只能包含字母、数字、点、下划线和短横线",
    "zh-TW": "名稱只能包含字母、數字、點、下劃線和短橫線",
    en: "A name may only contain letters, digits, dots, underscores and hyphens.",
  },
  "agent_config.name_length": {
    "zh-CN": "名称必须包含 1 到 64 个字符",
    "zh-TW": "名稱必須包含 1 到 64 個字符",
    en: "A name must have between 1 and 64 characters.",
  },
  "agent_config.not_object": {
    "zh-CN": "Pi 配置必须是 JSON 对象",
    "zh-TW": "Pi 配置必須是 JSON 對象",
    en: "The Pi configuration must be a JSON object.",
  },
  "agent_config.project_missing": {
    "zh-CN": "项目目录不存在",
    "zh-TW": "項目目錄不存在",
    en: "The project directory does not exist.",
  },
  "agent_config.project_path_invalid": {
    "zh-CN": "项目路径无效",
    "zh-TW": "項目路徑無效",
    en: "The project path is not valid.",
  },
  "agent_config.project_path_invalid_chars": {
    "zh-CN": "项目路径包含非法字符",
    "zh-TW": "項目路徑包含非法字符",
    en: "The project path contains invalid characters.",
  },
  "agent_config.project_path_required": {
    "zh-CN": "项目范围需要项目路径",
    "zh-TW": "項目范圍需要項目路徑",
    en: "A project scope needs a project path.",
  },
  "agent_config.skill_content_length": {
    "zh-CN": "Skill 内容必须包含 1 到 65536 字节",
    "zh-TW": "Skill 內容必須包含 1 到 65536 字節",
    en: "Skill content must have between 1 and 65536 bytes.",
  },
  "agent_config.skill_description_length": {
    "zh-CN": "Skill 描述必须包含 1 到 512 个字符",
    "zh-TW": "Skill 描述必須包含 1 到 512 個字符",
    en: "The Skill description must have between 1 and 512 characters.",
  },
  "app.main_window_closed": {
    "zh-CN": "主窗口已关闭",
    "zh-TW": "主窗口已關閉",
    en: "The main window is closed.",
  },
  "app.startup.not_ready": {
    "zh-CN": "应用正在初始化，请稍后重试",
    "zh-TW": "應用正在初始化，請稍後重試",
    en: "The app is still starting up. Try again in a moment.",
  },
  "app.startup.still_initializing": {
    "zh-CN": "初始化仍在进行，请稍后重试",
    "zh-TW": "初始化仍在進行，請稍後重試",
    en: "Startup is still in progress. Try again in a moment.",
  },
  "diagnostics.export_dialog_title": {
    "zh-CN": "导出诊断报告（新文件）",
    "zh-TW": "導出診斷報告（新文件）",
    en: "Export diagnostics report (new file)",
  },
  "diagnostics.export_in_progress": {
    "zh-CN": "诊断报告正在导出",
    "zh-TW": "診斷報告正在導出",
    en: "A diagnostics report export is already in progress.",
  },
  "diagnostics.export_location_unsupported": {
    "zh-CN": "不支持该报告保存位置",
    "zh-TW": "不支持該報告保存位置",
    en: "That location is not supported for saving a report.",
  },
  "diagnostics.export_task_failed": {
    "zh-CN": "诊断导出任务失败",
    "zh-TW": "診斷導出任務失敗",
    en: "The diagnostics export task failed.",
  },
  "diagnostics.journal_unavailable": {
    "zh-CN": "诊断记录暂不可用",
    "zh-TW": "診斷記錄暫不可用",
    en: "The diagnostics journal is not available right now.",
  },
  "diagnostics.main_window_only": {
    "zh-CN": "诊断仅允许主窗口访问",
    "zh-TW": "診斷僅允許主窗口訪問",
    en: "Diagnostics can only be accessed from the main window.",
  },
  "diagnostics.preview_expired": {
    "zh-CN": "诊断预览已失效，请刷新后重新导出",
    "zh-TW": "診斷預覽已失效，請刷新後重新導出",
    en: "The diagnostics preview has expired. Refresh and export again.",
  },
  "diagnostics.report_create_failed": {
    "zh-CN": "无法创建报告，请选择可写目录和新的文件名；不会覆盖已有文件",
    "zh-TW": "無法創建報告，請選擇可寫目錄和新的文件名；不會覆蓋已有文件",
    en: "Could not create the report. Choose a writable directory and a new file name; existing files are never overwritten.",
  },
  "diagnostics.report_device_path": {
    "zh-CN": "报告不支持设备路径",
    "zh-TW": "報告不支持設備路徑",
    en: "Reports do not support device paths.",
  },
  "diagnostics.report_file_name_invalid": {
    "zh-CN": "报告文件名无效",
    "zh-TW": "報告文件名無效",
    en: "The report file name is not valid.",
  },
  "diagnostics.report_generate_failed": {
    "zh-CN": "无法生成诊断报告",
    "zh-TW": "無法生成診斷報告",
    en: "Could not generate the diagnostics report.",
  },
  "diagnostics.report_path_required": {
    "zh-CN": "报告需要完整的普通文件路径",
    "zh-TW": "報告需要完整的普通文件路徑",
    en: "A report needs a complete regular file path.",
  },
  "diagnostics.report_write_incomplete": {
    "zh-CN": "报告写入未完成，目标可能保留不完整文件，请核对后另选文件名",
    "zh-TW": "報告寫入未完成，目標可能保留不完整文件，請核對後另選文件名",
    en: "The report was not written completely. The target may hold an incomplete file; check it and choose a different file name.",
  },
  "dsh.start.failed": {
    "zh-CN": "DSH 启动失败（{error}）：{detail}",
    "zh-TW": "DSH 啟動失敗（{error}）：{detail}",
    en: "Failed to start DSH ({error}): {detail}",
  },
  "external_editor.absolute_exe_required": {
    "zh-CN": "请选择对应编辑器的绝对 .exe 路径",
    "zh-TW": "請選擇對應編輯器的絕對 .exe 路徑",
    en: "Select the absolute path to the editor executable (.exe).",
  },
  "external_editor.launch_failed": {
    "zh-CN": "无法启动外部编辑器: {error}",
    "zh-TW": "無法啟動外部編輯器: {error}",
    en: "Failed to launch the external editor: {error}",
  },
  "external_editor.line_column_range": {
    "zh-CN": "文件行列必须在 1 到 100000000 之间",
    "zh-TW": "文件行列必須在 1 到 100000000 之間",
    en: "The file line and column must be between 1 and 100000000.",
  },
  "external_editor.not_configured": {
    "zh-CN": "尚未配置外部编辑器，请先打开设置",
    "zh-TW": "尚未配置外部編輯器，請先打開設置",
    en: "No external editor is configured yet. Open Settings first.",
  },
  "external_editor.parent_missing": {
    "zh-CN": "编辑器路径无父目录",
    "zh-TW": "編輯器路徑無父目錄",
    en: "The editor path has no parent directory.",
  },
  "external_editor.path_not_unicode": {
    "zh-CN": "文件路径不是有效 Unicode",
    "zh-TW": "文件路徑不是有效 Unicode",
    en: "The file path is not valid Unicode.",
  },
  "files.access_failed": {
    "zh-CN": "无法访问项目路径：{detail}",
    "zh-TW": "無法訪問項目路徑：{detail}",
    en: "Failed to access the project path: {detail}",
  },
  "files.delete_failed": {
    "zh-CN": "无法删除选定副本：{detail}",
    "zh-TW": "無法刪除選定副本：{detail}",
    en: "Failed to delete the selected copy: {detail}",
  },
  "files.identity_failed": {
    "zh-CN": "无法识别固定路径：{detail}",
    "zh-TW": "無法識別固定路徑：{detail}",
    en: "Failed to identify the pinned path: {detail}",
  },
  "files.list_dir_failed": {
    "zh-CN": "无法列出目录：{detail}",
    "zh-TW": "無法列出目錄：{detail}",
    en: "Failed to list the directory: {detail}",
  },
  "files.no_file_handle": {
    "zh-CN": "项目路径没有可用的文件句柄",
    "zh-TW": "項目路徑沒有可用的文件句柄",
    en: "The project path has no usable file handle.",
  },
  "files.ripgrep_missing": {
    "zh-CN": "未找到 ripgrep (rg.exe)，请配置系统 PATH 后重试",
    "zh-TW": "未找到 ripgrep (rg.exe)，請配置系統 PATH 後重試",
    en: "ripgrep (rg.exe) was not found. Configure the system PATH and try again.",
  },
  "files.save.backup_unverified": {
    "zh-CN": "文件已替换，但无法核验旧版本：{detail}。已保留恢复副本，请先核对。",
    "zh-TW": "文件已替換，但無法核驗舊版本：{detail}。已保留恢復副本，請先核對。",
    en: "The file was replaced, but the previous version could not be verified: {detail}. A recovery copy was kept; check it first.",
  },
  "files.save.busy": {
    "zh-CN": "另一文件正在保存，请稍后重试。",
    "zh-TW": "另一文件正在保存，請稍後重試。",
    en: "Another file is being saved. Try again in a moment.",
  },
  "files.save.destination_created": {
    "zh-CN": "另存目标刚被其他程序创建，本次未覆盖。",
    "zh-TW": "另存目標剛被其他程序創建，本次未覆蓋。",
    en: "The save-as target was just created by another program; it was not overwritten.",
  },
  "files.save.destination_exists": {
    "zh-CN": "另存目标已存在，本次未覆盖。请选择新文件名。",
    "zh-TW": "另存目標已存在，本次未覆蓋。請選擇新文件名。",
    en: "The save-as target already exists; it was not overwritten. Choose a new file name.",
  },
  "files.save.external_change": {
    "zh-CN": "文件已被外部修改，本次未写入。请比较或重载后再保存。",
    "zh-TW": "文件已被外部修改，本次未寫入。請比較或重載後再保存。",
    en: "The file changed on disk since it was opened; nothing was written. Compare or reload it before saving again.",
  },
  "files.save.publish_failed": {
    "zh-CN": "无法发布新文件，本次未覆盖现有文件：{detail}",
    "zh-TW": "無法發布新文件，本次未覆蓋現有文件：{detail}",
    en: "Failed to publish the new file; the existing file was not overwritten: {detail}",
  },
  "files.save.read_only": {
    "zh-CN": "文件为只读，本次未写入。",
    "zh-TW": "文件為只讀，本次未寫入。",
    en: "The file is read-only; nothing was written.",
  },
  "files.save.replace_conflict": {
    "zh-CN": "替换期间检测到外部修改：目标可能已是本次内容，外部版本已保留在恢复副本。请比较后处理，不要直接重试。",
    "zh-TW": "替換期間檢測到外部修改：目標可能已是本次內容，外部版本已保留在恢復副本。請比較後處理，不要直接重試。",
    en: "An external change was detected during the replace: the target may already hold this content, and the external version was kept in a recovery copy. Compare them before acting, and do not simply retry.",
  },
  "files.save.replace_mismatch": {
    "zh-CN": "文件已替换，但新内容或身份核验不一致。旧版本副本已保留，请比较后处理。",
    "zh-TW": "文件已替換，但新內容或身份核驗不一致。舊版本副本已保留，請比較後處理。",
    en: "The file was replaced, but the new content or identity did not verify. The previous version was kept as a copy; compare them before acting.",
  },
  "files.save.replace_unverified": {
    "zh-CN": "替换结果未确认：{detail}。目标或恢复路径可能已变化，请先核对；未自动重试或删除恢复文件。",
    "zh-TW": "替換結果未確認：{detail}。目標或恢復路徑可能已變化，請先核對；未自動重試或刪除恢復文件。",
    en: "The replace outcome is unconfirmed: {detail}. The target or recovery path may have changed; check first. Nothing was retried and no recovery file was deleted automatically.",
  },
  "files.save.saved": {
    "zh-CN": "已保存，旧版本保留在恢复副本中。",
    "zh-TW": "已保存，舊版本保留在恢復副本中。",
    en: "Saved; the previous version was kept as a recovery copy.",
  },
  "files.save.saved_as_new": {
    "zh-CN": "已另存为新文件。",
    "zh-TW": "已另存為新文件。",
    en: "Saved as a new file.",
  },
  "files.save.target_missing": {
    "zh-CN": "文件已删除或移动，请重载或另存为新文件。",
    "zh-TW": "文件已刪除或移動，請重載或另存為新文件。",
    en: "The save target no longer exists; reload the file or save it as a new file.",
  },
  "files.save.temp_create_failed": {
    "zh-CN": "无法创建保存临时文件：{detail}",
    "zh-TW": "無法創建保存臨時文件：{detail}",
    en: "Failed to create the temporary save file: {detail}",
  },
  "files.save.temp_write_failed": {
    "zh-CN": "无法写入保存临时文件：{detail}",
    "zh-TW": "無法寫入保存臨時文件：{detail}",
    en: "Failed to write the temporary save file: {detail}",
  },
  "files.save.unverified": {
    "zh-CN": "新文件已发布，但内容或身份核验不一致。请保留草稿并检查目标文件，不要直接重试。",
    "zh-TW": "新文件已發布，但內容或身份核驗不一致。請保留草稿並檢查目標文件，不要直接重試。",
    en: "The new file was published, but its content or identity did not verify. Keep your draft and inspect the target file; do not simply retry.",
  },
  "files.search_enumerate_failed": {
    "zh-CN": "无法枚举搜索文件，请检查目录权限",
    "zh-TW": "無法枚舉搜索文件，請檢查目錄權限",
    en: "Failed to enumerate files for the search. Check the directory permissions.",
  },
  "files.search_failed": {
    "zh-CN": "搜索文件失败，请检查目录权限或文件是否可读",
    "zh-TW": "搜索文件失敗，請檢查目錄權限或文件是否可讀",
    en: "The file search failed. Check the directory permissions or whether the files can be read.",
  },
  "files.symlink_unsupported": {
    "zh-CN": "不支持符号链接与重解析点",
    "zh-TW": "不支持符號鏈接與重解析點",
    en: "Symbolic links and reparse points are not supported.",
  },
  "files.unexpected_entry_type": {
    "zh-CN": "项目条目类型不符合预期",
    "zh-TW": "項目條目類型不符合預期",
    en: "The project entry has an unexpected type.",
  },
  "files.watch_capacity": {
    "zh-CN": "项目监听重复或已达容量上限",
    "zh-TW": "項目監聽重復或已達容量上限",
    en: "The file watcher has reached its capacity.",
  },
  "files.watch_closed": {
    "zh-CN": "项目监听已结束，请重新连接",
    "zh-TW": "項目監聽已結束，請重新連接",
    en: "The file watch has ended; connect again to resume watching.",
  },
  "files.watch_create_failed": {
    "zh-CN": "无法创建项目文件监听",
    "zh-TW": "無法創建項目文件監聽",
    en: "Failed to create the project file watcher.",
  },
  "files.watch_id_invalid": {
    "zh-CN": "项目监听标识无效",
    "zh-TW": "項目監聽標識無效",
    en: "The file watch ID is not valid.",
  },
  "files.watch_ping_failed": {
    "zh-CN": "项目监听续期失败",
    "zh-TW": "項目監聽續期失敗",
    en: "Failed to renew the file watch.",
  },
  "files.watch_spawn_failed": {
    "zh-CN": "项目监听启动任务失败",
    "zh-TW": "項目監聽啟動任務失敗",
    en: "Failed to start the file watch task.",
  },
  "files.watch_start_failed": {
    "zh-CN": "无法监听项目目录，请检查权限或使用手动刷新",
    "zh-TW": "無法監聽項目目錄，請檢查權限或使用手動刷新",
    en: "Failed to watch the project directory. Check the permissions or use manual refresh.",
  },
  "files.watch_stop_failed": {
    "zh-CN": "项目监听停止任务失败",
    "zh-TW": "項目監聽停止任務失敗",
    en: "Failed to stop the file watch task.",
  },
  "files.watch_thread_failed": {
    "zh-CN": "无法启动项目文件监听线程",
    "zh-TW": "無法啟動項目文件監聽線程",
    en: "Failed to start the project file watch thread.",
  },
  "files.watch_unavailable": {
    "zh-CN": "项目监听暂不可用",
    "zh-TW": "項目監聽暫不可用",
    en: "The file watch is not available right now.",
  },
  "managed.dsh_missing": {
    "zh-CN": "托管 DSH 运行时未安装，请在“设置 → 运行时与更新”安装 DSH",
    "zh-TW": "托管 DSH 運行時未安裝，請在“設置 → 運行時與更新”安裝 DSH",
    en: "The managed DSH runtime is not installed. Install DSH in Settings → “Runtime & Updates”.",
  },
  "managed.native_task_unsupported": {
    "zh-CN": "当前稳定版仅支持 DeepPi 托管任务，本机会话记录已保留",
    "zh-TW": "當前穩定版僅支持 DeepPi 托管任務，本機會話記錄已保留",
    en: "The current stable release only supports DeepPi managed tasks; the local session record was kept.",
  },
  "managed.node_missing": {
    "zh-CN": "托管 Node 运行时未安装，请在“设置 → 运行时与更新”安装 Node",
    "zh-TW": "托管 Node 運行時未安裝，請在“設置 → 運行時與更新”安裝 Node",
    en: "The managed Node runtime is not installed. Install Node in Settings → “Runtime & Updates”.",
  },
  "managed.npm_missing": {
    "zh-CN": "托管 npm 不可用，请在“设置 → 运行时与更新”修复 Node 运行时",
    "zh-TW": "托管 npm 不可用，請在“設置 → 運行時與更新”修復 Node 運行時",
    en: "The managed npm is not available. Repair the Node runtime in Settings → “Runtime & Updates”.",
  },
  "managed.pi_config_unavailable": {
    "zh-CN": "任务绑定的 Pi 配置目录不可用",
    "zh-TW": "任務綁定的 Pi 配置目錄不可用",
    en: "The Pi configuration directory bound to the task is not available.",
  },
  "managed.pi_missing": {
    "zh-CN": "托管 Pi 运行时未安装，请在“设置 → 运行时与更新”安装 Pi",
    "zh-TW": "托管 Pi 運行時未安裝，請在“設置 → 運行時與更新”安裝 Pi",
    en: "The managed Pi runtime is not installed. Install Pi in Settings → “Runtime & Updates”.",
  },
  "managed.runtime_dir_create_failed": {
    "zh-CN": "无法创建运行时目录 {path}：{error}。Pi/DSH 运行时安装在 DeepPi 安装目录下的 runtimes 子文件夹，请把 DeepPi 安装到可写的位置（不要安装在受保护的系统目录）",
    "zh-TW": "無法創建運行時目錄 {path}：{error}。Pi/DSH 運行時安裝在 DeepPi 安裝目錄下的 runtimes 子文件夹，請把 DeepPi 安裝到可寫的位置（不要安裝在受保護的系統目錄）",
    en: "Failed to create the runtime directory {path}: {error}. Pi/DSH runtimes are installed in the runtimes subfolder of the DeepPi install directory, so install DeepPi in a writable location (not a protected system directory).",
  },
  "native.config.invalid_json": {
    "zh-CN": "本机 Pi 配置不是有效 JSON",
    "zh-TW": "本機 Pi 配置不是有效 JSON",
    en: "The local Pi configuration is not valid JSON.",
  },
  "native.config.not_object": {
    "zh-CN": "本机 Pi 配置必须是 JSON 对象",
    "zh-TW": "本機 Pi 配置必須是 JSON 對象",
    en: "The local Pi configuration must be a JSON object.",
  },
  "native.config.read_failed": {
    "zh-CN": "无法读取本机 Pi 配置",
    "zh-TW": "無法讀取本機 Pi 配置",
    en: "Failed to read the local Pi configuration.",
  },
  "native.config.too_large": {
    "zh-CN": "本机 Pi 配置超过 1 MiB",
    "zh-TW": "本機 Pi 配置超過 1 MiB",
    en: "The local Pi configuration exceeds 1 MiB.",
  },
  "native.session.cwd_invalid": {
    "zh-CN": "会话项目目录无效",
    "zh-TW": "會話項目目錄無效",
    en: "The session project directory is not valid.",
  },
  "native.session.directory_unavailable": {
    "zh-CN": "会话目录不可用",
    "zh-TW": "會話目錄不可用",
    en: "The session directory is not available.",
  },
  "native.session.head_incomplete": {
    "zh-CN": "会话头不完整或超过上限",
    "zh-TW": "會話頭不完整或超過上限",
    en: "The session header is incomplete or exceeds the size limit.",
  },
  "native.session.head_invalid": {
    "zh-CN": "会话头无效",
    "zh-TW": "會話頭無效",
    en: "The session header is not valid.",
  },
  "native.session.head_read_failed": {
    "zh-CN": "无法读取会话头",
    "zh-TW": "無法讀取會話頭",
    en: "Failed to read the session header.",
  },
  "native.session.home_unresolved": {
    "zh-CN": "无法解析本机会话目录",
    "zh-TW": "無法解析本機會話目錄",
    en: "Could not resolve the local session directory.",
  },
  "native.session.id_invalid": {
    "zh-CN": "会话 ID 无效",
    "zh-TW": "會話 ID 無效",
    en: "The session ID is not valid.",
  },
  "native.session.id_mismatch": {
    "zh-CN": "会话文件已被替换，ID 不匹配",
    "zh-TW": "會話文件已被替換，ID 不匹配",
    en: "The session file was replaced; its ID does not match.",
  },
  "native.session.metadata_failed": {
    "zh-CN": "无法读取本机会话元数据",
    "zh-TW": "無法讀取本機會話元數據",
    en: "Failed to read the local session metadata.",
  },
  "native.session.not_a_file": {
    "zh-CN": "不是会话文件",
    "zh-TW": "不是會話文件",
    en: "This is not a session file.",
  },
  "native.session.open_failed": {
    "zh-CN": "无法读取本机 Pi 会话",
    "zh-TW": "無法讀取本機 Pi 會話",
    en: "Failed to read the local Pi session.",
  },
  "native.session.outside_configured_roots": {
    "zh-CN": "Pi 会话文件不在已配置的会话目录中",
    "zh-TW": "Pi 會話文件不在已配置的會話目錄中",
    en: "The Pi session file is not inside a configured session directory.",
  },
  "native.session.parent_missing": {
    "zh-CN": "会话路径缺少父目录",
    "zh-TW": "會話路徑缺少父目錄",
    en: "The session path has no parent directory.",
  },
  "native.session.project_changed": {
    "zh-CN": "会话文件所属项目已变化",
    "zh-TW": "會話文件所屬項目已變化",
    en: "The project the session file belongs to has changed.",
  },
  "native.session.project_unavailable": {
    "zh-CN": "会话项目目录不可用",
    "zh-TW": "會話項目目錄不可用",
    en: "The session project directory is not available.",
  },
  "native.session.reported_file_mismatch": {
    "zh-CN": "Pi 返回的会话文件与已绑定文件不一致",
    "zh-TW": "Pi 返回的會話文件與已綁定文件不一致",
    en: "The session file reported by Pi does not match the bound file.",
  },
  "native.session.reported_id_mismatch": {
    "zh-CN": "Pi 返回的会话 ID 与任务不一致",
    "zh-TW": "Pi 返回的會話 ID 與任務不一致",
    en: "The session ID reported by Pi does not match the task.",
  },
  "native.session.reported_path_invalid": {
    "zh-CN": "Pi 返回的会话文件路径无效",
    "zh-TW": "Pi 返回的會話文件路徑無效",
    en: "The session file path reported by Pi is not valid.",
  },
  "native.session.tail_read_failed": {
    "zh-CN": "无法读取会话尾",
    "zh-TW": "無法讀取會話尾",
    en: "Failed to read the session tail.",
  },
  "native.session.task_project_unavailable": {
    "zh-CN": "任务项目目录不可用",
    "zh-TW": "任務項目目錄不可用",
    en: "The task project directory is not available.",
  },
  "native.session.time_invalid": {
    "zh-CN": "会话时间无效",
    "zh-TW": "會話時間無效",
    en: "The session timestamp is not valid.",
  },
  "native.session.time_out_of_range": {
    "zh-CN": "会话时间超出范围",
    "zh-TW": "會話時間超出范圍",
    en: "The session timestamp is out of range.",
  },
  "native.session.unsupported_format": {
    "zh-CN": "不支持的 Pi 会话格式",
    "zh-TW": "不支持的 Pi 會話格式",
    en: "Unsupported Pi session format.",
  },
  "native.session.write_in_progress": {
    "zh-CN": "会话正在写入，稍后重试",
    "zh-TW": "會話正在寫入，稍後重試",
    en: "The session is being written; try again later.",
  },
  "recovery.busy": {
    "zh-CN": "文件正在处理，请稍后重试。",
    "zh-TW": "文件正在處理，請稍後重試。",
    en: "The file is being processed. Try again in a moment.",
  },
  "recovery.cleanup_error": {
    "zh-CN": "副本已处理，但记录清理失败，请刷新核对：{detail}",
    "zh-TW": "副本已處理，但記錄清理失敗，請刷新核對：{detail}",
    en: "The copy was processed, but the record could not be cleaned up. Refresh to check: {detail}",
  },
  "recovery.cleanup_failed": {
    "zh-CN": "副本处理后无法清理恢复记录。",
    "zh-TW": "副本處理後無法清理恢復記錄。",
    en: "The recovery record could not be cleaned up after the copy was processed.",
  },
  "recovery.copy_changed": {
    "zh-CN": "副本已变化，请刷新后重新核对。",
    "zh-TW": "副本已變化，請刷新後重新核對。",
    en: "The copy has changed. Refresh and check it again.",
  },
  "recovery.copy_changed_no_delete": {
    "zh-CN": "副本已变化，本次未删除。请刷新后核对。",
    "zh-TW": "副本已變化，本次未刪除。請刷新後核對。",
    en: "The copy has changed; nothing was deleted. Refresh and check it again.",
  },
  "recovery.detail_copy_missing": {
    "zh-CN": "副本路径当前不存在，可移除此记录。",
    "zh-TW": "副本路徑當前不存在，可移除此記錄。",
    en: "The copy path does not exist right now; this record can be removed.",
  },
  "recovery.detail_project_changed": {
    "zh-CN": "项目或副本目录身份已变化或无法核验，未访问此记录对应的副本。",
    "zh-TW": "項目或副本目錄身份已變化或無法核驗，未訪問此記錄對應的副本。",
    en: "The project or copy directory identity has changed or could not be verified, so the copy for this record was not accessed.",
  },
  "recovery.detail_record_invalid": {
    "zh-CN": "恢复记录格式无效。",
    "zh-TW": "恢復記錄格式無效。",
    en: "The recovery record has an invalid format.",
  },
  "recovery.file_name_invalid": {
    "zh-CN": "恢复副本文件名无效。",
    "zh-TW": "恢復副本文件名無效。",
    en: "The recovery copy file name is not valid.",
  },
  "recovery.id_format_invalid": {
    "zh-CN": "恢复副本标识格式无效。",
    "zh-TW": "恢復副本標識格式無效。",
    en: "The recovery copy ID has an invalid format.",
  },
  "recovery.id_invalid": {
    "zh-CN": "恢复副本标识无效。",
    "zh-TW": "恢復副本標識無效。",
    en: "The recovery copy ID is not valid.",
  },
  "recovery.id_mismatch": {
    "zh-CN": "恢复记录标识不匹配。",
    "zh-TW": "恢復記錄標識不匹配。",
    en: "The recovery record ID does not match.",
  },
  "recovery.limit_reached": {
    "zh-CN": "恢复记录已达上限，请先处理旧记录。",
    "zh-TW": "恢復記錄已達上限，請先處理舊記錄。",
    en: "The recovery limit has been reached; deal with older records first.",
  },
  "recovery.lock_unavailable": {
    "zh-CN": "恢复记录锁不可用。",
    "zh-TW": "恢復記錄鎖不可用。",
    en: "The recovery record lock is not available.",
  },
  "recovery.pagination_invalid": {
    "zh-CN": "无效的恢复记录分页。",
    "zh-TW": "無效的恢復記錄分頁。",
    en: "Invalid recovery record pagination.",
  },
  "recovery.record_invalid": {
    "zh-CN": "无效的恢复副本记录。",
    "zh-TW": "無效的恢復副本記錄。",
    en: "Invalid recovery copy record.",
  },
  "recovery.record_missing": {
    "zh-CN": "找不到此项目的恢复记录。",
    "zh-TW": "找不到此項目的恢復記錄。",
    en: "No recovery record was found for this project.",
  },
  "recovery.scope_changed": {
    "zh-CN": "项目或副本目录身份已变化。",
    "zh-TW": "項目或副本目錄身份已變化。",
    en: "The project or copy directory identity has changed.",
  },
  "runtime.dsh.busy": {
    "zh-CN": "请先关闭 DSH，再切换 DSH runtime",
    "zh-TW": "請先關閉 DSH，再切換 DSH runtime",
    en: "Close DSH before switching the DSH runtime.",
  },
  "runtime.dsh.pinned": {
    "zh-CN": "上游已有 {latest}；该版本尚未通过 DeepPi 兼容验证，暂时固定 {pinned}",
    "zh-TW": "上游已有 {latest}；該版本尚未通過 DeepPi 兼容驗證，暫時固定 {pinned}",
    en: "Upstream has {latest}; that version has not passed DeepPi compatibility verification yet, so {pinned} stays pinned for now.",
  },
  "runtime.dsh.unsupported": {
    "zh-CN": "DSH {requested} 尚未通过 DeepPi 兼容验证，请安装 {verified}",
    "zh-TW": "DSH {requested} 尚未通過 DeepPi 兼容驗證，請安裝 {verified}",
    en: "DSH {requested} has not passed DeepPi compatibility verification yet. Install {verified}.",
  },
  "runtime.dshmarket.no_backup": {
    "zh-CN": "没有可用的 dshmarket 回滚版本",
    "zh-TW": "沒有可用的 dshmarket 回滾版本",
    en: "No dshmarket rollback version is available.",
  },
  "runtime.install.checking_registry": {
    "zh-CN": "查询 registry 最新版本",
    "zh-TW": "查詢 registry 最新版本",
    en: "Checking the registry for the latest version",
  },
  "runtime.install.component_not_ready": {
    "zh-CN": "该组件当前不可更新，请先满足其运行时兼容条件",
    "zh-TW": "該組件當前不可更新，請先滿足其運行時兼容條件",
    en: "This component cannot be updated right now. Satisfy its runtime compatibility requirements first.",
  },
  "runtime.install.not_latest": {
    "zh-CN": "只能安装刚从 registry 验证的最新版本",
    "zh-TW": "只能安裝剛從 registry 驗證的最新版本",
    en: "Only the latest version just verified against the registry can be installed.",
  },
  "runtime.install.preparing": {
    "zh-CN": "准备安装 {version}",
    "zh-TW": "準備安裝 {version}",
    en: "Preparing to install {version}",
  },
  "runtime.node.archive_invalid": {
    "zh-CN": "Node 归档无效：{detail}",
    "zh-TW": "Node 歸檔無效：{detail}",
    en: "The Node archive is not valid: {detail}",
  },
  "runtime.node.archive_open_failed": {
    "zh-CN": "无法打开 Node 归档：{detail}",
    "zh-TW": "無法打開 Node 歸檔：{detail}",
    en: "Failed to open the Node archive: {detail}",
  },
  "runtime.node.archive_read_failed": {
    "zh-CN": "Node 归档读取失败：{detail}",
    "zh-TW": "Node 歸檔讀取失敗：{detail}",
    en: "Failed to read the Node archive: {detail}",
  },
  "runtime.node.archive_unsafe_path": {
    "zh-CN": "Node 归档包含非法路径",
    "zh-TW": "Node 歸檔包含非法路徑",
    en: "The Node archive contains an unsafe path.",
  },
  "runtime.node.busy": {
    "zh-CN": "请先停止所有 Pi 任务并关闭 DSH，再修复 Node 运行时",
    "zh-TW": "請先停止所有 Pi 任務並關閉 DSH，再修復 Node 運行時",
    en: "Stop all Pi tasks and close DSH before repairing the Node runtime.",
  },
  "runtime.node.checksum_entry_missing": {
    "zh-CN": "官方校验文件缺少 {file}",
    "zh-TW": "官方校驗文件缺少 {file}",
    en: "The official checksum file has no entry for {file}.",
  },
  "runtime.node.checksum_mismatch": {
    "zh-CN": "Node 归档的 SHA-256 校验失败",
    "zh-TW": "Node 歸檔的 SHA-256 校驗失敗",
    en: "The Node archive failed its SHA-256 checksum check.",
  },
  "runtime.node.download_failed": {
    "zh-CN": "下载失败：{detail}",
    "zh-TW": "下載失敗：{detail}",
    en: "Download failed: {detail}",
  },
  "runtime.node.download_file_create_failed": {
    "zh-CN": "无法创建下载文件：{detail}",
    "zh-TW": "無法創建下載文件：{detail}",
    en: "Failed to create the download file: {detail}",
  },
  "runtime.node.download_flush_failed": {
    "zh-CN": "下载落盘失败：{detail}",
    "zh-TW": "下載落盤失敗：{detail}",
    en: "Failed to flush the download to disk: {detail}",
  },
  "runtime.node.download_http_failed": {
    "zh-CN": "下载返回 HTTP {status}",
    "zh-TW": "下載返回 HTTP {status}",
    en: "The download returned HTTP {status}",
  },
  "runtime.node.download_read_failed": {
    "zh-CN": "下载响应读取失败：{detail}",
    "zh-TW": "下載響應讀取失敗：{detail}",
    en: "Failed to read the download response: {detail}",
  },
  "runtime.node.download_stream_read_failed": {
    "zh-CN": "下载读取失败：{detail}",
    "zh-TW": "下載讀取失敗：{detail}",
    en: "Failed to read the download: {detail}",
  },
  "runtime.node.download_write_failed": {
    "zh-CN": "下载写入失败：{detail}",
    "zh-TW": "下載寫入失敗：{detail}",
    en: "Failed to write the download: {detail}",
  },
  "runtime.node.downloading_package": {
    "zh-CN": "下载 Node 官方发行包",
    "zh-TW": "下載 Node 官方發行包",
    en: "Downloading the official Node distribution",
  },
  "runtime.node.exe_missing": {
    "zh-CN": "Node 运行时缺少 node.exe",
    "zh-TW": "Node 運行時缺少 node.exe",
    en: "The Node runtime is missing node.exe.",
  },
  "runtime.node.fixed_version": {
    "zh-CN": "Node 运行时只能安装内置固定版本 {version}",
    "zh-TW": "Node 運行時只能安裝內置固定版本 {version}",
    en: "The Node runtime can only install the bundled fixed version {version}.",
  },
  "runtime.node.npm_missing": {
    "zh-CN": "Node 运行时缺少 npm.cmd（安装 Pi/DSH 需要托管 npm）",
    "zh-TW": "Node 運行時缺少 npm.cmd（安裝 Pi/DSH 需要托管 npm）",
    en: "The Node runtime is missing npm.cmd (a managed npm is required to install Pi/DSH).",
  },
  "runtime.node.verify_failed": {
    "zh-CN": "Node 运行时启动验证失败",
    "zh-TW": "Node 運行時啟動驗證失敗",
    en: "The Node runtime failed its startup verification.",
  },
  "runtime.node.verifying_checksum": {
    "zh-CN": "校验 SHA-256",
    "zh-TW": "校驗 SHA-256",
    en: "Verifying SHA-256",
  },
  "runtime.node.version_mismatch": {
    "zh-CN": "Node 版本不符：期望 {expected}，实际 {actual}",
    "zh-TW": "Node 版本不符：期望 {expected}，實際 {actual}",
    en: "Node version mismatch: expected {expected}, got {actual}",
  },
  "runtime.npm.downloading": {
    "zh-CN": "正在从 {registry} 下载依赖（已 {seconds}s，可随时取消）",
    "zh-TW": "正在從 {registry} 下載依賴（已 {seconds}s，可隨時取消）",
    en: "Downloading dependencies from {registry} ({seconds}s elapsed, cancellable at any time)",
  },
  "runtime.npm.fallback": {
    "zh-CN": "{previous} 失败，改用 {next} 重试",
    "zh-TW": "{previous} 失敗，改用 {next} 重試",
    en: "{previous} failed; retrying with {next}",
  },
  "runtime.pi.busy": {
    "zh-CN": "请先停止所有 Pi Session，再切换 Pi runtime",
    "zh-TW": "請先停止所有 Pi Session，再切換 Pi runtime",
    en: "Stop all Pi sessions before switching the Pi runtime.",
  },
  "runtime.rollback.not_available": {
    "zh-CN": "该组件没有可回滚的独立运行时",
    "zh-TW": "該組件沒有可回滾的獨立運行時",
    en: "This component has no separate runtime to roll back to.",
  },
};
