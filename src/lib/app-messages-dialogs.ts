/**
 * 后端消息分片：原生对话框文案。
 *
 * 这些文案由前端按当前语言取好，经 `appConfirmDialog()` 组装后传给后端
 * （见 `src-tauri/src/dialog_text.rs` 的 `ConfirmDialog`）；`{name}` 占位符
 * **原样保留**，由后端在确认时刻用实测值渲染。
 *
 * 键的约定：`<code>.title` / `<code>.message` / `<code>.confirm` / `<code>.cancel`，
 * 词汇另加 `<code>.term.<name>`。
 */
export const APP_MESSAGES_DIALOGS: Record<string, import("./app-messages").AppMessageText> = {
  "git.trust.dialog.title": {
    "zh-CN": "信任项目并读取 Git 变更",
    "zh-TW": "信任專案並讀取 Git 變更",
    en: "Trust this project and read Git changes",
  },
  "git.trust.dialog.message": {
    "zh-CN":
      "允许本机 Git 读取此项目吗？\n\n项目：{project}\n工作树：{worktree}\nGit 元数据：{git_dir}\n公共元数据：{common_dir}\n\n变更与差异仅限项目目录。Git 会使用用户及仓库配置，可能运行配置的文件过滤器。仅对可信项目允许；此授权持续到应用退出。本入口不执行暂存、提交或推送。",
    "zh-TW":
      "允許本機 Git 讀取此專案嗎？\n\n專案：{project}\n工作樹：{worktree}\nGit 中繼資料：{git_dir}\n共用中繼資料：{common_dir}\n\n變更與差異僅限專案目錄。Git 會使用使用者及倉庫設定，可能執行設定的檔案篩選器。僅對可信任的專案允許；此授權持續到應用程式結束。本入口不執行暫存、提交或推送。",
    en: "Allow the local Git to read this project?\n\nProject: {project}\nWorktree: {worktree}\nGit metadata: {git_dir}\nCommon metadata: {common_dir}\n\nChanges and diffs are limited to the project directory. Git uses the user and repository configuration and may run configured file filters. Allow this only for trusted projects; this authorization lasts until the application exits. This entry point does not stage, commit, or push.",
  },
  "git.trust.dialog.confirm": {
    "zh-CN": "信任并读取",
    "zh-TW": "信任並讀取",
    en: "Trust and read",
  },
  "git.trust.dialog.cancel": {
    "zh-CN": "取消",
    "zh-TW": "取消",
    en: "Cancel",
  },
  "git.index.dialog.stage.title": {
    "zh-CN": "暂存所选文件",
    "zh-TW": "暫存所選檔案",
    en: "Stage the selected files",
  },
  "git.index.dialog.stage.message": {
    "zh-CN":
      "将执行时的工作区内容加入暂存区，不修改工作区文件。\n\n工作树：{worktree}\n\n{paths}\n\nGit 会使用已信任的仓库配置和文件过滤器。",
    "zh-TW":
      "將執行時的工作區內容加入暫存區，不修改工作區檔案。\n\n工作樹：{worktree}\n\n{paths}\n\nGit 會使用已信任的倉庫設定和檔案篩選器。",
    en: "Adds the working-tree contents as of execution time to the index without modifying working-tree files.\n\nWorktree: {worktree}\n\n{paths}\n\nGit uses the trusted repository configuration and file filters.",
  },
  "git.index.dialog.stage.confirm": {
    "zh-CN": "暂存所选文件",
    "zh-TW": "暫存所選檔案",
    en: "Stage the selected files",
  },
  "git.index.dialog.stage.cancel": {
    "zh-CN": "取消",
    "zh-TW": "取消",
    en: "Cancel",
  },
  "git.index.dialog.unstage.title": {
    "zh-CN": "取消所选文件暂存",
    "zh-TW": "取消所選檔案暫存",
    en: "Unstage the selected files",
  },
  "git.index.dialog.unstage.message": {
    "zh-CN":
      "将所选索引路径恢复到当前提交，保留工作区文件。\n\n工作树：{worktree}\n\n{paths}\n\nGit 会使用已信任的仓库配置和文件过滤器。",
    "zh-TW":
      "將所選索引路徑還原到目前提交，保留工作區檔案。\n\n工作樹：{worktree}\n\n{paths}\n\nGit 會使用已信任的倉庫設定和檔案篩選器。",
    en: "Restores the selected index paths to the current commit, keeping the working-tree files.\n\nWorktree: {worktree}\n\n{paths}\n\nGit uses the trusted repository configuration and file filters.",
  },
  "git.index.dialog.unstage.confirm": {
    "zh-CN": "取消所选文件暂存",
    "zh-TW": "取消所選檔案暫存",
    en: "Unstage the selected files",
  },
  "git.index.dialog.unstage.cancel": {
    "zh-CN": "取消",
    "zh-TW": "取消",
    en: "Cancel",
  },
  "git.index.dialog.resolve.title": {
    "zh-CN": "标记冲突已解决",
    "zh-TW": "標記衝突已解決",
    en: "Mark conflicts as resolved",
  },
  "git.index.dialog.resolve.message": {
    "zh-CN":
      "以执行时的工作区内容替换冲突索引；缺失文件按删除处理。请先确认冲突已解决。\n\n工作树：{worktree}\n\n{paths}\n\nGit 会使用已信任的仓库配置和文件过滤器。",
    "zh-TW":
      "以執行時的工作區內容取代衝突索引；缺少的檔案以刪除處理。請先確認衝突已解決。\n\n工作樹：{worktree}\n\n{paths}\n\nGit 會使用已信任的倉庫設定和檔案篩選器。",
    en: "Replaces the conflicted index entries with the working-tree contents as of execution time; missing files are treated as deletions. Confirm that the conflicts are resolved first.\n\nWorktree: {worktree}\n\n{paths}\n\nGit uses the trusted repository configuration and file filters.",
  },
  "git.index.dialog.resolve.confirm": {
    "zh-CN": "标记冲突已解决",
    "zh-TW": "標記衝突已解決",
    en: "Mark conflicts as resolved",
  },
  "git.index.dialog.resolve.cancel": {
    "zh-CN": "取消",
    "zh-TW": "取消",
    en: "Cancel",
  },
  "git.commit.dialog.title": {
    "zh-CN": "确认提交暂存变更",
    "zh-TW": "確認提交暫存變更",
    en: "Confirm committing staged changes",
  },
  "git.commit.dialog.message": {
    "zh-CN":
      "工作树：{worktree}\n目标：{target}\n作者：{author}\n签名：{signature}\n暂存树：{tree}\n\n{paths}\n\n提交说明摘要：\n{summary}\n\n本次不运行 Git hooks，也不会推送。",
    "zh-TW":
      "工作樹：{worktree}\n目標：{target}\n作者：{author}\n簽名：{signature}\n暫存樹：{tree}\n\n{paths}\n\n提交說明摘要：\n{summary}\n\n本次不執行 Git hooks，也不會推送。",
    en: "Worktree: {worktree}\nTarget: {target}\nAuthor: {author}\nSignature: {signature}\nStaged tree: {tree}\n\n{paths}\n\nCommit message summary:\n{summary}\n\nThis run does not execute Git hooks and does not push.",
  },
  "git.commit.dialog.confirm": {
    "zh-CN": "提交",
    "zh-TW": "提交",
    en: "Commit",
  },
  "git.commit.dialog.cancel": {
    "zh-CN": "取消",
    "zh-TW": "取消",
    en: "Cancel",
  },
  "git.commit.dialog.term.signature_on": {
    "zh-CN": "启用",
    "zh-TW": "啟用",
    en: "On",
  },
  "git.commit.dialog.term.signature_off": {
    "zh-CN": "关闭",
    "zh-TW": "關閉",
    en: "Off",
  },
  "git.commit.dialog.term.more_paths": {
    "zh-CN": "…共 {count} 个路径，完整范围已在应用中列出",
    "zh-TW": "…共 {count} 個路徑，完整範圍已在應用程式中列出",
    en: "…{count} paths in total; the full list is shown in the application",
  },
  "git.push.dialog.title": {
    "zh-CN": "确认推送",
    "zh-TW": "確認推送",
    en: "Confirm the push",
  },
  "git.push.dialog.message": {
    "zh-CN":
      "工作树：{worktree}\n远程：{remote}\n目的地：{destination}\n目标：{target}\n源提交：{source}\n\n推送会传输此提交及其可达历史，不限于项目子目录。\n仅更新上述分支，不强推、不附带标签、不递归推送子模块、不运行客户端 hooks。\n使用系统现有凭据；本入口不交互登录、不签署推送证书。",
    "zh-TW":
      "工作樹：{worktree}\n遠端：{remote}\n目的地：{destination}\n目標：{target}\n來源提交：{source}\n\n推送會傳輸此提交及其可達歷史，不限於專案子目錄。\n僅更新上述分支，不強推、不附加標籤、不遞迴推送子模組、不執行用戶端 hooks。\n使用系統現有憑證；本入口不互動登入、不簽署推送憑證。",
    en: "Worktree: {worktree}\nRemote: {remote}\nDestination: {destination}\nTarget: {target}\nSource commit: {source}\n\nThe push transfers this commit and its reachable history, not only the project subdirectory.\nIt updates only the branch above: no force push, no tags, no recursive submodule push, and no client hooks.\nSystem credentials are used; this entry point does not log in interactively or sign push certificates.",
  },
  "git.push.dialog.confirm": {
    "zh-CN": "推送",
    "zh-TW": "推送",
    en: "Push",
  },
  "git.push.dialog.cancel": {
    "zh-CN": "取消",
    "zh-TW": "取消",
    en: "Cancel",
  },
  "git.sync.dialog.title": {
    "zh-CN": "确认同步远程跟踪引用",
    "zh-TW": "確認同步遠端追蹤參照",
    en: "Confirm syncing remote-tracking references",
  },
  "git.sync.dialog.message": {
    "zh-CN":
      "工作树：{worktree}\n远程：{remote}\n目的地：{destination}\n目标分支：{branch}\n查询提交：{query}\n\n本地跟踪引用：\n{references}\n\n下载此提交的可达对象，并将以上引用更新到查询提交，包括远程回退。\n不合并、不变基本地分支，不修改工作区、索引、标签或上游设置；不运行客户端 hooks。\n远程查询未返回目标时不删除本地引用。使用现有凭据，不交互登录。",
    "zh-TW":
      "工作樹：{worktree}\n遠端：{remote}\n目的地：{destination}\n目標分支：{branch}\n查詢提交：{query}\n\n本機追蹤參照：\n{references}\n\n下載此提交的可達物件，並將以上參照更新到查詢提交，包括遠端回退。\n不合併、不重定基底本機分支，不修改工作區、索引、標籤或上游設定；不執行用戶端 hooks。\n遠端查詢未傳回目標時不刪除本機參照。使用現有憑證，不互動登入。",
    en: "Worktree: {worktree}\nRemote: {remote}\nDestination: {destination}\nTarget branch: {branch}\nQueried commit: {query}\n\nLocal tracking references:\n{references}\n\nDownloads the objects reachable from this commit and updates the references above to the queried commit, including remote rewinds.\nDoes not merge or rebase the local branch, and does not modify the worktree, index, tags, or upstream settings; does not run client hooks.\nLocal references are not deleted when the remote query does not return the target. Existing credentials are used; no interactive login.",
  },
  "git.sync.dialog.confirm": {
    "zh-CN": "同步",
    "zh-TW": "同步",
    en: "Sync",
  },
  "git.sync.dialog.cancel": {
    "zh-CN": "取消",
    "zh-TW": "取消",
    en: "Cancel",
  },
  "git.sync.dialog.term.new_ref": {
    "zh-CN": "新建",
    "zh-TW": "新增",
    en: "New",
  },
  "recovery.delete.dialog.permanent.title": {
    "zh-CN": "永久删除副本",
    "zh-TW": "永久刪除副本",
    en: "Permanently delete the copy",
  },
  "recovery.delete.dialog.permanent.message": {
    "zh-CN": "项目：{project}\n副本：{copy}\n原目标：{target}\n\n永久删除选定副本，不修改工作文件。",
    "zh-TW": "專案：{project}\n副本：{copy}\n原目標：{target}\n\n永久刪除選定的副本，不修改工作檔案。",
    en: "Project: {project}\nCopy: {copy}\nOriginal target: {target}\n\nPermanently deletes the selected copy without modifying working files.",
  },
  "recovery.delete.dialog.permanent.confirm": {
    "zh-CN": "永久删除副本",
    "zh-TW": "永久刪除副本",
    en: "Permanently delete the copy",
  },
  "recovery.delete.dialog.permanent.cancel": {
    "zh-CN": "取消",
    "zh-TW": "取消",
    en: "Cancel",
  },
  "recovery.delete.dialog.record_only.title": {
    "zh-CN": "仅移除记录",
    "zh-TW": "僅移除記錄",
    en: "Remove the record only",
  },
  "recovery.delete.dialog.record_only.message": {
    "zh-CN": "项目：{project}\n副本：{copy}\n原目标：{target}\n\n仅移除恢复记录，不删除或修改任何文件。",
    "zh-TW": "專案：{project}\n副本：{copy}\n原目標：{target}\n\n僅移除復原記錄，不刪除或修改任何檔案。",
    en: "Project: {project}\nCopy: {copy}\nOriginal target: {target}\n\nRemoves only the recovery record; no files are deleted or modified.",
  },
  "recovery.delete.dialog.record_only.confirm": {
    "zh-CN": "仅移除记录",
    "zh-TW": "僅移除記錄",
    en: "Remove the record only",
  },
  "recovery.delete.dialog.record_only.cancel": {
    "zh-CN": "取消",
    "zh-TW": "取消",
    en: "Cancel",
  },
  "recovery.restore.dialog.title": {
    "zh-CN": "另存恢复文件",
    "zh-TW": "另存復原檔案",
    en: "Save the recovered file as",
  },
  "recovery.restore.dialog.message": {
    "zh-CN": "项目：{project}\n副本：{copy}\n新文件：{new_path}\n\n只创建新文件，不覆盖已有文件；原副本保留。",
    "zh-TW": "專案：{project}\n副本：{copy}\n新檔案：{new_path}\n\n只建立新檔案，不覆寫既有檔案；原副本保留。",
    en: "Project: {project}\nCopy: {copy}\nNew file: {new_path}\n\nOnly a new file is created; existing files are not overwritten, and the original copy is kept.",
  },
  "recovery.restore.dialog.confirm": {
    "zh-CN": "另存",
    "zh-TW": "另存",
    en: "Save as",
  },
  "recovery.restore.dialog.cancel": {
    "zh-CN": "取消",
    "zh-TW": "取消",
    en: "Cancel",
  },
  "app.dialog_text_invalid": {
    "zh-CN": "确认对话框文案无效（{field}）",
    "zh-TW": "確認對話框文案無效（{field}）",
    en: "Invalid confirmation dialog text ({field}).",
  },
};
