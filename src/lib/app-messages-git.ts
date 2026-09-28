/** 后端消息分片：Git 操作。消息码由 Rust 模块迁移而来，三种语言均已翻译。 */
export const APP_MESSAGES_GIT: Record<string, import("./app-messages").AppMessageText> = {
  "git.commit.branch_encoding": {
    "zh-CN": "Git 分支名称不是有效 UTF-8",
    "zh-TW": "Git 分支名稱不是有效 UTF-8",
    en: "The Git branch name is not valid UTF-8",
  },
  "git.commit.detached_head_missing": {
    "zh-CN": "分离 HEAD 没有原始提交",
    "zh-TW": "分離 HEAD 沒有原始提交",
    en: "A detached HEAD has no original commit",
  },
  "git.commit.detail_unconfirmed": {
    "zh-CN": "引用更新未确认成功；可能存在分支变化、引用锁或 Git 版本不支持。请刷新并核对提交 ID，不会自动重试",
    "zh-TW": "引用更新未確認成功；可能存在分支變化、引用鎖或 Git 版本不支持。請刷新並核對提交 ID，不會自動重試",
    en: "The reference update was not confirmed; the branch may have changed, a reference lock may be held, or the Git version may not support it. Refresh and verify the commit ID; this will not be retried automatically",
  },
  "git.commit.detail_verified_after_error": {
    "zh-CN": "命令异常后已核实引用更新成功",
    "zh-TW": "命令異常後已核實引用更新成功",
    en: "The reference update was verified as successful after the command failed",
  },
  "git.commit.head_changed": {
    "zh-CN": "准备期间 HEAD 已变化，请重新准备提交",
    "zh-TW": "準備期間 HEAD 已變化，請重新準備提交",
    en: "HEAD changed while preparing the commit. Prepare the commit again.",
  },
  "git.commit.head_changed_abort": {
    "zh-CN": "HEAD 类型或内容已变化，引用事务已中止",
    "zh-TW": "HEAD 類型或內容已變化，引用事務已中止",
    en: "The type or contents of HEAD changed; the reference transaction was aborted",
  },
  "git.commit.head_changed_before_publish": {
    "zh-CN": "发布前 HEAD 已变化，未更新引用，请重新准备提交",
    "zh-TW": "發布前 HEAD 已變化，未更新引用，請重新準備提交",
    en: "HEAD changed before publishing; the reference was not updated. Prepare the commit again.",
  },
  "git.commit.head_parse_failed": {
    "zh-CN": "无法解析当前提交",
    "zh-TW": "無法解析當前提交",
    en: "Unable to parse the current commit",
  },
  "git.commit.head_read_failed": {
    "zh-CN": "无法读取 Git HEAD",
    "zh-TW": "無法讀取 Git HEAD",
    en: "Unable to read Git HEAD",
  },
  "git.commit.head_too_large": {
    "zh-CN": "Git HEAD 超过读取上限",
    "zh-TW": "Git HEAD 超過讀取上限",
    en: "Git HEAD exceeds the read limit",
  },
  "git.commit.identity_invalid": {
    "zh-CN": "Git 身份格式异常",
    "zh-TW": "Git 身份格式異常",
    en: "The Git identity has an invalid format",
  },
  "git.commit.identity_missing": {
    "zh-CN": "无法确定 Git 提交身份，请配置 user.name 和 user.email",
    "zh-TW": "無法確定 Git 提交身份，請配置 user.name 和 user.email",
    en: "Unable to determine the Git commit identity. Configure user.name and user.email.",
  },
  "git.commit.invalid_oid": {
    "zh-CN": "Git 对象 ID 格式异常",
    "zh-TW": "Git 對象 ID 格式異常",
    en: "The Git object ID has an invalid format",
  },
  "git.commit.message_invalid": {
    "zh-CN": "提交说明不能为空、不能包含 NUL，且不得超过 64 KiB",
    "zh-TW": "提交說明不能為空、不能包含 NUL，且不得超過 64 KiB",
    en: "The commit message cannot be empty, contain NUL, or exceed 64 KiB",
  },
  "git.commit.no_staged": {
    "zh-CN": "没有可提交的暂存变更",
    "zh-TW": "沒有可提交的暫存變更",
    en: "There are no staged changes to commit.",
  },
  "git.commit.object_failed": {
    "zh-CN": "未能创建提交对象，请检查 Git 身份和签名工具；未更新分支",
    "zh-TW": "未能創建提交對象，請檢查 Git 身份和簽名工具；未更新分支",
    en: "The commit object could not be created. Check the Git identity and signing tool; the branch was not updated.",
  },
  "git.commit.paths_encoding": {
    "zh-CN": "暂存路径不是有效 UTF-8",
    "zh-TW": "暫存路徑不是有效 UTF-8",
    en: "Staged paths are not valid UTF-8",
  },
  "git.commit.paths_incomplete": {
    "zh-CN": "暂存路径列表不完整",
    "zh-TW": "暫存路徑列表不完整",
    en: "The list of staged paths is incomplete",
  },
  "git.commit.paths_outside_scope": {
    "zh-CN": "存在项目范围以外的暂存文件，请从完整仓库项目中提交",
    "zh-TW": "存在項目范圍以外的暫存文件，請從完整倉庫項目中提交",
    en: "Staged files exist outside the project scope. Commit from the full repository project.",
  },
  "git.commit.prepare_window_required": {
    "zh-CN": "准备提交需要主窗口",
    "zh-TW": "準備提交需要主窗口",
    en: "Preparing a commit requires the main window",
  },
  "git.commit.reference_not_published": {
    "zh-CN": "未发布引用",
    "zh-TW": "未發布引用",
    en: "The reference was not published",
  },
  "git.commit.scope_changed": {
    "zh-CN": "提交范围已变化，请重新准备",
    "zh-TW": "提交范圍已變化，請重新準備",
    en: "The commit scope changed. Prepare the commit again.",
  },
  "git.commit.sequence_in_progress": {
    "zh-CN": "仓库正在 merge、rebase 或其他序列操作中；请先在外部 Git 工具中完成该流程",
    "zh-TW": "倉庫正在 merge、rebase 或其他序列操作中；請先在外部 Git 工具中完成該流程",
    en: "The repository is in the middle of a merge, rebase, or other sequenced operation. Complete it in an external Git tool first.",
  },
  "git.commit.signing_invalid": {
    "zh-CN": "Git 签名配置异常",
    "zh-TW": "Git 簽名配置異常",
    en: "The Git signing configuration is invalid",
  },
  "git.commit.signing_read_failed": {
    "zh-CN": "无法读取 Git 签名配置",
    "zh-TW": "無法讀取 Git 簽名配置",
    en: "Unable to read the Git signing configuration",
  },
  "git.commit.staged_changed": {
    "zh-CN": "暂存内容、分支或提交身份已变化，请重新准备并确认提交范围",
    "zh-TW": "暫存內容、分支或提交身份已變化，請重新準備並確認提交范圍",
    en: "Staged contents, the branch, or the commit identity changed. Prepare the commit again and confirm the commit scope.",
  },
  "git.commit.staged_range_failed": {
    "zh-CN": "无法完整读取暂存范围，未准备提交",
    "zh-TW": "無法完整讀取暫存范圍，未準備提交",
    en: "The staged range could not be read completely; the commit was not prepared",
  },
  "git.commit.transaction_exited": {
    "zh-CN": "Git 引用事务提前退出",
    "zh-TW": "Git 引用事務提前退出",
    en: "The Git reference transaction exited early",
  },
  "git.commit.transaction_not_exited": {
    "zh-CN": "Git 引用事务未及时退出",
    "zh-TW": "Git 引用事務未及時退出",
    en: "The Git reference transaction did not exit in time",
  },
  "git.commit.transaction_response": {
    "zh-CN": "Git 引用事务响应异常",
    "zh-TW": "Git 引用事務響應異常",
    en: "Unexpected response from the Git reference transaction",
  },
  "git.commit.transaction_timeout": {
    "zh-CN": "Git 引用事务等待超时",
    "zh-TW": "Git 引用事務等待超時",
    en: "Timed out waiting for the Git reference transaction",
  },
  "git.commit.tree_failed": {
    "zh-CN": "无法生成暂存树，请先解决索引冲突",
    "zh-TW": "無法生成暫存樹，請先解決索引沖突",
    en: "Unable to create the staged tree. Resolve the index conflicts first.",
  },
  "git.commit.unsupported_encoding": {
    "zh-CN": "Git 返回了不支持的编码",
    "zh-TW": "Git 返回了不支持的編碼",
    en: "Git returned an unsupported encoding",
  },
  "git.commit.unsupported_head": {
    "zh-CN": "不支持的 HEAD 引用，未执行提交",
    "zh-TW": "不支持的 HEAD 引用，未執行提交",
    en: "Unsupported HEAD reference; the commit was not created",
  },
  "git.commit.window_required": {
    "zh-CN": "Git 提交需要主窗口",
    "zh-TW": "Git 提交需要主窗口",
    en: "Committing with Git requires the main window",
  },
  "git.diff.group_changed": {
    "zh-CN": "文件已不属于所选变更分组，请刷新后重试",
    "zh-TW": "文件已不屬於所選變更分組，請刷新後重試",
    en: "The file no longer belongs to the selected change group. Refresh and try again.",
  },
  "git.diff.read_failed": {
    "zh-CN": "Git 差异读取失败（退出码 {code}），请刷新或在可信终端检查仓库",
    "zh-TW": "Git 差異讀取失敗（退出碼 {code}），請刷新或在可信終端檢查倉庫",
    en: "Failed to read the Git diff (exit code {code}). Refresh or check the repository in a trusted terminal.",
  },
  "git.diff.status_changed": {
    "zh-CN": "文件状态已变化，请刷新变更列表",
    "zh-TW": "文件狀態已變化，請刷新變更列表",
    en: "The file status changed. Refresh the change list.",
  },
  "git.diff.window_required": {
    "zh-CN": "Git 差异读取需要主窗口",
    "zh-TW": "Git 差異讀取需要主窗口",
    en: "Reading a Git diff requires the main window",
  },
  "git.diff.worktree_too_large": {
    "zh-CN": "工作区文件超过 8 MiB，请在外部 Git 工具中查看",
    "zh-TW": "工作區文件超過 8 MiB，請在外部 Git 工具中查看",
    en: "The worktree file exceeds 8 MiB. View it in an external Git tool.",
  },
  "git.index.group_changed": {
    "zh-CN": "文件不属于此操作对应的变更分组，请刷新后重试",
    "zh-TW": "文件不屬於此操作對應的變更分組，請刷新後重試",
    en: "The file does not belong to the change group for this operation. Refresh and try again.",
  },
  "git.index.lock_failed": {
    "zh-CN": "无法取得 Git 索引锁，未执行写入；请检查其他 Git 操作（不会删除已有锁）: {detail}",
    "zh-TW": "無法取得 Git 索引鎖，未執行寫入；請檢查其他 Git 操作（不會刪除已有鎖）: {detail}",
    en: "Could not acquire the Git index lock; nothing was written. Check for other Git operations (existing locks are not removed): {detail}",
  },
  "git.index.new_file_missing": {
    "zh-CN": "新文件已消失，请刷新后重试",
    "zh-TW": "新文件已消失，請刷新後重試",
    en: "The new file has disappeared. Refresh and try again.",
  },
  "git.index.not_generated": {
    "zh-CN": "Git 未生成临时索引，未执行写入",
    "zh-TW": "Git 未生成臨時索引，未執行寫入",
    en: "Git did not create a temporary index; nothing was written",
  },
  "git.index.operation_failed": {
    "zh-CN": "Git 索引操作未完成（退出码 {code}），真实索引未更新；请检查文件状态或过滤器",
    "zh-TW": "Git 索引操作未完成（退出碼 {code}），真實索引未更新；請檢查文件狀態或過濾器",
    en: "The Git index operation did not complete (exit code {code}); the real index was not updated. Check the file status or filters.",
  },
  "git.index.publish_failed": {
    "zh-CN": "无法发布 Git 索引，原索引未替换: {detail}",
    "zh-TW": "無法發布 Git 索引，原索引未替換: {detail}",
    en: "Unable to publish the Git index; the original index was not replaced: {detail}",
  },
  "git.index.read_failed": {
    "zh-CN": "无法读取 Git 索引: {detail}",
    "zh-TW": "無法讀取 Git 索引: {detail}",
    en: "Unable to read the Git index: {detail}",
  },
  "git.index.rename_outside_project": {
    "zh-CN": "重命名跨越项目边界，请从完整仓库项目中操作",
    "zh-TW": "重命名跨越項目邊界，請從完整倉庫項目中操作",
    en: "The rename crosses the project boundary. Perform it from the full repository project.",
  },
  "git.index.rename_source_missing": {
    "zh-CN": "重命名缺少源路径",
    "zh-TW": "重命名缺少源路徑",
    en: "The rename is missing its source path",
  },
  "git.index.status_changed": {
    "zh-CN": "文件状态已变化，请刷新并重新选择，不会自动重试写入",
    "zh-TW": "文件狀態已變化，請刷新並重新選擇，不會自動重試寫入",
    en: "The file status changed. Refresh and select again; the write will not be retried automatically.",
  },
  "git.index.too_large": {
    "zh-CN": "Git 索引超过 32 MiB，未执行写入",
    "zh-TW": "Git 索引超過 32 MiB，未執行寫入",
    en: "The Git index exceeds 32 MiB; nothing was written",
  },
  "git.index.transaction_finished": {
    "zh-CN": "Git 索引事务已结束",
    "zh-TW": "Git 索引事務已結束",
    en: "The Git index transaction has ended",
  },
  "git.index.window_required": {
    "zh-CN": "Git 写入需要主窗口",
    "zh-TW": "Git 寫入需要主窗口",
    en: "Writing with Git requires the main window",
  },
  "git.index.worktree_too_large": {
    "zh-CN": "工作区文件超过 8 MiB，未执行暂存",
    "zh-TW": "工作區文件超過 8 MiB，未執行暫存",
    en: "The worktree file exceeds 8 MiB; it was not staged",
  },
  "git.operation.budget_exhausted": {
    "zh-CN": "Git 读取已超过总等待预算，请重试",
    "zh-TW": "Git 讀取已超過總等待預算，請重試",
    en: "Git reads exceeded the overall wait budget. Try again.",
  },
  "git.operation.lock_poisoned": {
    "zh-CN": "Git 操作锁已失效，请重启应用",
    "zh-TW": "Git 操作鎖已失效，請重啟應用",
    en: "The Git operation lock is no longer valid. Restart the application.",
  },
  "git.operation.queue_full": {
    "zh-CN": "Git 请求队列已满，请稍后重试",
    "zh-TW": "Git 請求隊列已滿，請稍後重試",
    en: "The Git request queue is full. Try again later.",
  },
  "git.operation.request_reused": {
    "zh-CN": "Git 请求已取消、完成或正在运行，请使用新的请求 ID",
    "zh-TW": "Git 請求已取消、完成或正在運行，請使用新的請求 ID",
    en: "The Git request was already cancelled or completed, or is still running. Use a new request ID.",
  },
  "git.push.ambiguous_destination": {
    "zh-CN": "推送地址不再是唯一目的地，未执行推送",
    "zh-TW": "推送地址不再是唯一目的地，未執行推送",
    en: "The push destination is no longer unique; the push was not performed",
  },
  "git.push.branch_invalid": {
    "zh-CN": "目标分支名称无效",
    "zh-TW": "目標分支名稱無效",
    en: "The target branch name is invalid",
  },
  "git.push.config_changed": {
    "zh-CN": "推送配置或源提交已变化，请重新加载",
    "zh-TW": "推送配置或源提交已變化，請重新加載",
    en: "The push configuration or source commit changed. Reload and try again.",
  },
  "git.push.config_encoding": {
    "zh-CN": "Git 推送配置不是有效 UTF-8",
    "zh-TW": "Git 推送配置不是有效 UTF-8",
    en: "The Git push configuration is not valid UTF-8",
  },
  "git.push.config_read_failed": {
    "zh-CN": "无法完整读取 Git 推送配置",
    "zh-TW": "無法完整讀取 Git 推送配置",
    en: "Unable to read the complete Git push configuration",
  },
  "git.push.destination_changed": {
    "zh-CN": "远程推送地址已变化，请重新加载并选择目的地",
    "zh-TW": "遠程推送地址已變化，請重新加載並選擇目的地",
    en: "The remote push URL changed. Reload and choose a destination.",
  },
  "git.push.destination_invalid": {
    "zh-CN": "不支持或包含敏感参数的推送地址；请使用无内嵌凭据的 HTTPS、SSH 或本地绝对路径",
    "zh-TW": "不支持或包含敏感參數的推送地址；請使用無內嵌憑據的 HTTPS、SSH 或本地絕對路徑",
    en: "Unsupported push URL, or it contains sensitive parameters. Use HTTPS, SSH, or a local absolute path without embedded credentials.",
  },
  "git.push.detail.pushed": {
    "zh-CN": "远程已接受所选提交。",
    "zh-TW": "遠程已接受所選提交。",
    en: "The remote accepted the selected commit.",
  },
  "git.push.detail.rejected": {
    "zh-CN": "远程拒绝推送，请检查非快进、分支保护或服务器策略；未强推。",
    "zh-TW": "遠程拒絕推送，請檢查非快進、分支保護或服務器策略；未強推。",
    en: "The remote rejected the push. Check for a non-fast-forward update, branch protection, or server policy; no force push was performed.",
  },
  "git.push.detail.unknown": {
    "zh-CN": "推送结果未确认。可能是认证、网络、取消或服务端错误；请核对远程目标，勿直接重复推送。",
    "zh-TW": "推送結果未確認。可能是認證、網絡、取消或服務端錯誤；請核對遠程目標，勿直接重復推送。",
    en: "The push result is unconfirmed. It may be an authentication, network, cancellation, or server error. Verify the remote target and do not push again immediately.",
  },
  "git.push.detail.up_to_date": {
    "zh-CN": "远程目标已是所选提交。",
    "zh-TW": "遠程目標已是所選提交。",
    en: "The remote target already points to the selected commit.",
  },
  "git.push.encode_failed": {
    "zh-CN": "无法编码推送地址",
    "zh-TW": "無法編碼推送地址",
    en: "Unable to encode the push URL",
  },
  "git.push.no_commits": {
    "zh-CN": "仓库尚无提交，不能推送",
    "zh-TW": "倉庫尚無提交，不能推送",
    en: "The repository has no commits yet and cannot be pushed",
  },
  "git.push.remote_address_changed": {
    "zh-CN": "远程地址已变化，不能用新地址核对旧推送",
    "zh-TW": "遠程地址已變化，不能用新地址核對舊推送",
    en: "The remote URL changed; a previous push cannot be verified against the new URL",
  },
  "git.push.remote_encoding": {
    "zh-CN": "远程响应不是有效 UTF-8",
    "zh-TW": "遠程響應不是有效 UTF-8",
    en: "The remote response is not valid UTF-8",
  },
  "git.push.remote_incomplete": {
    "zh-CN": "远程响应不完整，未确认目标状态",
    "zh-TW": "遠程響應不完整，未確認目標狀態",
    en: "The remote response is incomplete; the target state was not confirmed",
  },
  "git.push.remote_name_invalid": {
    "zh-CN": "不支持的 Git 远程名称",
    "zh-TW": "不支持的 Git 遠程名稱",
    en: "Unsupported Git remote name",
  },
  "git.push.remote_ref_invalid": {
    "zh-CN": "远程引用格式异常",
    "zh-TW": "遠程引用格式異常",
    en: "The remote reference has an invalid format",
  },
  "git.push.remote_ref_out_of_range": {
    "zh-CN": "远程引用格式或范围异常",
    "zh-TW": "遠程引用格式或范圍異常",
    en: "The remote reference has an invalid format or is out of range",
  },
  "git.push.rewritten_destination": {
    "zh-CN": "地址重写改变了已确认的目的地，未执行推送",
    "zh-TW": "地址重寫改變了已確認的目的地，未執行推送",
    en: "URL rewriting changed the confirmed destination; the push was not performed",
  },
  "git.push.source_changed": {
    "zh-CN": "源提交、分支或推送目的地已变化，请重新确认",
    "zh-TW": "源提交、分支或推送目的地已變化，請重新確認",
    en: "The source commit, branch, or push destination changed. Confirm again.",
  },
  "git.push.targets_window_required": {
    "zh-CN": "推送配置需要主窗口",
    "zh-TW": "推送配置需要主窗口",
    en: "Push configuration requires the main window",
  },
  "git.push.temp_config_failed": {
    "zh-CN": "无法创建临时推送配置",
    "zh-TW": "無法創建臨時推送配置",
    en: "Unable to create a temporary push configuration",
  },
  "git.push.temp_config_write_failed": {
    "zh-CN": "无法写入临时推送配置",
    "zh-TW": "無法寫入臨時推送配置",
    en: "Unable to write the temporary push configuration",
  },
  "git.push.too_many_targets": {
    "zh-CN": "推送目的地超过 64 个，未展示不完整列表",
    "zh-TW": "推送目的地超過 64 個，未展示不完整列表",
    en: "There are more than 64 push destinations; the incomplete list is not shown",
  },
  "git.push.verify_failed": {
    "zh-CN": "远程核对失败或响应超限；不能判断目标是否存在，请检查认证和连接",
    "zh-TW": "遠程核對失敗或響應超限；不能判斷目標是否存在，請檢查認證和連接",
    en: "Remote verification failed or the response exceeded the limit; whether the target exists cannot be determined. Check authentication and connectivity.",
  },
  "git.push.verify_request_invalid": {
    "zh-CN": "无效的远程核对请求",
    "zh-TW": "無效的遠程核對請求",
    en: "Invalid remote verification request",
  },
  "git.push.verify_window_required": {
    "zh-CN": "远程核对需要主窗口",
    "zh-TW": "遠程核對需要主窗口",
    en: "Remote verification requires the main window",
  },
  "git.push.window_required": {
    "zh-CN": "Git 推送需要主窗口",
    "zh-TW": "Git 推送需要主窗口",
    en: "Pushing with Git requires the main window",
  },
  "git.repository.changed": {
    "zh-CN": "Git 仓库定位期间发生变化，请重新读取",
    "zh-TW": "Git 倉庫定位期間發生變化，請重新讀取",
    en: "The Git repository changed while it was being located. Read it again.",
  },
  "git.repository.git_missing": {
    "zh-CN": "未找到项目外的 Git (git.exe)，请检查系统 PATH",
    "zh-TW": "未找到項目外的 Git (git.exe)，請檢查系統 PATH",
    en: "Git (git.exe) was not found outside the project. Check the system PATH.",
  },
  "git.repository.locate_failed": {
    "zh-CN": "无法定位项目的 Git 工作树和元数据目录",
    "zh-TW": "無法定位項目的 Git 工作樹和元數據目錄",
    en: "Unable to locate the Git worktree and metadata directory for the project",
  },
  "git.repository.locate_format": {
    "zh-CN": "Git 仓库定位结果格式异常",
    "zh-TW": "Git 倉庫定位結果格式異常",
    en: "The Git repository location result has an invalid format",
  },
  "git.repository.marker_parent_missing": {
    "zh-CN": "Git 标识没有父目录",
    "zh-TW": "Git 標識沒有父目錄",
    en: "The Git marker has no parent directory",
  },
  "git.repository.marker_unreadable": {
    "zh-CN": "无法访问 Git 标识: {detail}",
    "zh-TW": "無法訪問 Git 標識: {detail}",
    en: "Unable to access the Git marker: {detail}",
  },
  "git.repository.not_a_worktree": {
    "zh-CN": "项目不属于 Git 工作树",
    "zh-TW": "項目不屬於 Git 工作樹",
    en: "The project is not inside a Git worktree",
  },
  "git.repository.not_trusted": {
    "zh-CN": "尚未授权读取 Git 变更，请使用刷新按钮确认信任",
    "zh-TW": "尚未授權讀取 Git 變更，請使用刷新按鈕確認信任",
    en: "Reading Git changes is not authorized yet. Use the refresh button to confirm trust.",
  },
  "git.repository.path_encoding": {
    "zh-CN": "Git 仓库路径不是有效 Unicode",
    "zh-TW": "Git 倉庫路徑不是有效 Unicode",
    en: "The Git repository path is not valid Unicode",
  },
  "git.repository.worktree_mismatch": {
    "zh-CN": "Git 工作树范围与所选项目不一致",
    "zh-TW": "Git 工作樹范圍與所選項目不一致",
    en: "The Git worktree scope does not match the selected project",
  },
  "git.status.outside_scope": {
    "zh-CN": "Git 返回了项目范围以外的变更",
    "zh-TW": "Git 返回了項目范圍以外的變更",
    en: "Git returned changes outside the project scope",
  },
  "git.status.read_failed": {
    "zh-CN": "Git 状态读取失败（退出码 {code}）；请在可信终端检查仓库权限或索引锁",
    "zh-TW": "Git 狀態讀取失敗（退出碼 {code}）；請在可信終端檢查倉庫權限或索引鎖",
    en: "Failed to read the Git status (exit code {code}). Check repository permissions or the index lock in a trusted terminal.",
  },
  "git.status.truncated": {
    "zh-CN": "Git 状态输出超过 64 KiB，未展示不完整列表；请在外部 Git 工具中查看",
    "zh-TW": "Git 狀態輸出超過 64 KiB，未展示不完整列表；請在外部 Git 工具中查看",
    en: "The Git status output exceeds 64 KiB; the incomplete list is not shown. View it in an external Git tool.",
  },
  "git.status.window_required": {
    "zh-CN": "Git 状态读取需要主窗口",
    "zh-TW": "Git 狀態讀取需要主窗口",
    en: "Reading Git status requires the main window",
  },
  "git.sync.config_encoding": {
    "zh-CN": "Git 同步配置或引用不是 UTF-8",
    "zh-TW": "Git 同步配置或引用不是 UTF-8",
    en: "The Git sync configuration or reference is not UTF-8",
  },
  "git.sync.config_output_truncated": {
    "zh-CN": "Git 同步配置或引用输出超限",
    "zh-TW": "Git 同步配置或引用輸出超限",
    en: "The Git sync configuration or reference output exceeded the limit",
  },
  "git.sync.config_read_failed": {
    "zh-CN": "无法读取 Git 同步配置或引用",
    "zh-TW": "無法讀取 Git 同步配置或引用",
    en: "Unable to read the Git sync configuration or reference",
  },
  "git.sync.destination_mismatch": {
    "zh-CN": "所选推送目的地与该远程的拉取地址不同，不能更新它的跟踪引用",
    "zh-TW": "所選推送目的地與該遠程的拉取地址不同，不能更新它的跟蹤引用",
    en: "The selected push destination differs from this remote's fetch URL; its tracking reference cannot be updated",
  },
  "git.sync.fetch_failed": {
    "zh-CN": "获取所选提交对象失败或输出超限，未发布跟踪引用",
    "zh-TW": "獲取所選提交對象失敗或輸出超限，未發布跟蹤引用",
    en: "Failed to fetch the selected commit object, or the output exceeded the limit; the tracking reference was not published",
  },
  "git.sync.fetch_mapping_incomplete": {
    "zh-CN": "不支持或不完整的 fetch 引用映射",
    "zh-TW": "不支持或不完整的 fetch 引用映射",
    en: "Unsupported or incomplete fetch reference mapping",
  },
  "git.sync.fetch_mapping_unsupported": {
    "zh-CN": "不支持的 fetch 引用映射",
    "zh-TW": "不支持的 fetch 引用映射",
    en: "Unsupported fetch reference mapping",
  },
  "git.sync.invalid_object_id": {
    "zh-CN": "跟踪引用的对象 ID 无效",
    "zh-TW": "跟蹤引用的對象 ID 無效",
    en: "The object ID of the tracking reference is invalid",
  },
  "git.sync.mapping_not_remote_tracking": {
    "zh-CN": "fetch 映射不是远程跟踪引用，不能从此入口改写本地分支或标签",
    "zh-TW": "fetch 映射不是遠程跟蹤引用，不能從此入口改寫本地分支或標簽",
    en: "The fetch mapping is not a remote-tracking reference; local branches and tags cannot be rewritten from here",
  },
  "git.sync.negative_mapping_invalid": {
    "zh-CN": "负 fetch 引用映射无效",
    "zh-TW": "負 fetch 引用映射無效",
    en: "Invalid negative fetch reference mapping",
  },
  "git.sync.no_mapping": {
    "zh-CN": "目标分支没有可用的远程跟踪映射，可能被负 refspec 排除",
    "zh-TW": "目標分支沒有可用的遠程跟蹤映射，可能被負 refspec 排除",
    en: "The target branch has no usable remote-tracking mapping; it may be excluded by a negative refspec",
  },
  "git.sync.not_a_commit": {
    "zh-CN": "远程目标不是提交对象，未发布跟踪引用",
    "zh-TW": "遠程目標不是提交對象，未發布跟蹤引用",
    en: "The remote target is not a commit object; the tracking reference was not published",
  },
  "git.sync.preview_changed": {
    "zh-CN": "远程目标、映射或本地跟踪引用已变化，请重新确认同步",
    "zh-TW": "遠程目標、映射或本地跟蹤引用已變化，請重新確認同步",
    en: "The remote target, mapping, or local tracking reference changed. Confirm the sync again.",
  },
  "git.sync.reference_not_published": {
    "zh-CN": "未发布跟踪引用",
    "zh-TW": "未發布跟蹤引用",
    en: "The tracking reference was not published",
  },
  "git.sync.references_changed": {
    "zh-CN": "跟踪引用或映射已变化，事务已中止",
    "zh-TW": "跟蹤引用或映射已變化，事務已中止",
    en: "The tracking reference or mapping changed; the transaction was aborted",
  },
  "git.sync.remote_target_missing": {
    "zh-CN": "远程查询未返回目标分支，不会删除本地跟踪引用",
    "zh-TW": "遠程查詢未返回目標分支，不會刪除本地跟蹤引用",
    en: "The remote did not return the requested target branch; the local tracking reference will not be deleted",
  },
  "git.sync.symbolic_reference": {
    "zh-CN": "跟踪引用已是符号引用，未改写其目标分支",
    "zh-TW": "跟蹤引用已是符號引用，未改寫其目標分支",
    en: "The tracking reference is already a symbolic reference; its target branch was not rewritten",
  },
  "git.sync.too_many_mappings": {
    "zh-CN": "远程跟踪映射超过 64 个",
    "zh-TW": "遠程跟蹤映射超過 64 個",
    en: "There are more than 64 remote-tracking mappings",
  },
  "git.sync.transaction_exited": {
    "zh-CN": "跟踪引用事务提前退出",
    "zh-TW": "跟蹤引用事務提前退出",
    en: "The tracking reference transaction exited early",
  },
  "git.sync.transaction_not_exited": {
    "zh-CN": "跟踪引用事务未及时退出",
    "zh-TW": "跟蹤引用事務未及時退出",
    en: "The tracking reference transaction did not exit in time",
  },
  "git.sync.transaction_response": {
    "zh-CN": "跟踪引用事务响应异常",
    "zh-TW": "跟蹤引用事務響應異常",
    en: "Unexpected response from the tracking reference transaction",
  },
  "git.sync.transaction_timeout": {
    "zh-CN": "跟踪引用事务超时",
    "zh-TW": "跟蹤引用事務超時",
    en: "Timed out waiting for the tracking reference transaction",
  },
  "git.sync.window_required": {
    "zh-CN": "同步远程跟踪引用需要主窗口",
    "zh-TW": "同步遠程跟蹤引用需要主窗口",
    en: "Syncing remote-tracking references requires the main window",
  },
};
