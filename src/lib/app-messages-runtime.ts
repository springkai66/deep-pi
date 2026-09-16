/** 后端消息分片：运行时与 DSH 服务（`src-tauri/src/runtime.rs`、`dsh.rs`）。 */
export const APP_MESSAGES_RUNTIME: Record<string, import("./app-messages").AppMessageText> = {
  "runtime.dsh.unsupported": {
    "zh-CN": "DSH {requested} 尚未通过 DeepPi 兼容验证，请安装 {verified}",
    "zh-TW": "DSH {requested} 尚未通過 DeepPi 相容驗證，請安裝 {verified}",
    en: "DSH {requested} has not passed DeepPi compatibility verification. Install {verified} instead.",
  },
  "runtime.dsh.pinned": {
    "zh-CN": "上游已有 {latest}；该版本尚未通过 DeepPi 兼容验证，暂时固定 {pinned}",
    "zh-TW": "上游已有 {latest}；該版本尚未通過 DeepPi 相容驗證，暫時固定 {pinned}",
    en: "Upstream has {latest}; that version has not passed DeepPi compatibility verification, so {pinned} stays pinned for now.",
  },
  "runtime.install.preparing": {
    "zh-CN": "准备安装 {version}",
    "zh-TW": "準備安裝 {version}",
    en: "Preparing to install {version}",
  },
  "runtime.install.checking_registry": {
    "zh-CN": "查询 registry 最新版本",
    "zh-TW": "查詢 registry 最新版本",
    en: "Checking the registry for the latest version",
  },
  "runtime.install.not_latest": {
    "zh-CN": "只能安装刚从 registry 验证的最新版本",
    "zh-TW": "只能安裝剛從 registry 驗證的最新版本",
    en: "Only the latest version just verified against the registry can be installed.",
  },
  "runtime.install.component_not_ready": {
    "zh-CN": "该组件当前不可更新，请先满足其运行时兼容条件",
    "zh-TW": "該元件目前無法更新，請先滿足其運行時相容條件",
    en: "This component cannot be updated right now. Satisfy its runtime compatibility requirements first.",
  },
  "runtime.rollback.not_available": {
    "zh-CN": "该组件没有可回滚的独立运行时",
    "zh-TW": "該元件沒有可還原的獨立運行時",
    en: "This component has no separate runtime to roll back.",
  },
  "runtime.dshmarket.no_backup": {
    "zh-CN": "没有可用的 dshmarket 回滚版本",
    "zh-TW": "沒有可用的 dshmarket 還原版本",
    en: "No dshmarket version is available for rollback.",
  },
  "runtime.pi.busy": {
    "zh-CN": "请先停止所有 Pi Session，再切换 Pi runtime",
    "zh-TW": "請先停止所有 Pi Session，再切換 Pi runtime",
    en: "Stop all Pi sessions before switching the Pi runtime.",
  },
  "runtime.node.busy": {
    "zh-CN": "请先停止所有 Pi 任务并关闭 DSH，再修复 Node 运行时",
    "zh-TW": "請先停止所有 Pi 任務並關閉 DSH，再修復 Node 運行時",
    en: "Stop all Pi tasks and shut down DSH before repairing the Node runtime.",
  },
  "runtime.dsh.busy": {
    "zh-CN": "请先关闭 DSH，再切换 DSH runtime",
    "zh-TW": "請先關閉 DSH，再切換 DSH runtime",
    en: "Shut down DSH before switching the DSH runtime.",
  },
  "runtime.npm.downloading": {
    "zh-CN": "正在从 {registry} 下载依赖（已 {seconds}s，可随时取消）",
    "zh-TW": "正在從 {registry} 下載相依套件（已 {seconds}s，可隨時取消）",
    en: "Downloading dependencies from {registry} ({seconds}s elapsed, cancel anytime)",
  },
  "runtime.npm.fallback": {
    "zh-CN": "{previous} 失败，改用 {next} 重试",
    "zh-TW": "{previous} 失敗，改用 {next} 重試",
    en: "{previous} failed. Retrying with {next}.",
  },
  "runtime.node.downloading_package": {
    "zh-CN": "下载 Node 官方发行包",
    "zh-TW": "下載 Node 官方發行包",
    en: "Downloading the official Node release",
  },
  "runtime.node.verifying_checksum": {
    "zh-CN": "校验 SHA-256",
    "zh-TW": "校驗 SHA-256",
    en: "Verifying SHA-256",
  },
  "runtime.node.download_failed": {
    "zh-CN": "下载失败：{detail}",
    "zh-TW": "下載失敗：{detail}",
    en: "Download failed: {detail}",
  },
  "runtime.node.download_http_failed": {
    "zh-CN": "下载返回 HTTP {status}",
    "zh-TW": "下載傳回 HTTP {status}",
    en: "Download returned HTTP {status}",
  },
  "runtime.node.download_read_failed": {
    "zh-CN": "下载响应读取失败：{detail}",
    "zh-TW": "下載回應讀取失敗：{detail}",
    en: "Failed to read the download response: {detail}",
  },
  "runtime.node.download_file_create_failed": {
    "zh-CN": "无法创建下载文件：{detail}",
    "zh-TW": "無法建立下載檔案：{detail}",
    en: "Could not create the download file: {detail}",
  },
  "runtime.node.download_stream_read_failed": {
    "zh-CN": "下载读取失败：{detail}",
    "zh-TW": "下載讀取失敗：{detail}",
    en: "Download read failed: {detail}",
  },
  "runtime.node.download_write_failed": {
    "zh-CN": "下载写入失败：{detail}",
    "zh-TW": "下載寫入失敗：{detail}",
    en: "Download write failed: {detail}",
  },
  "runtime.node.download_flush_failed": {
    "zh-CN": "下载落盘失败：{detail}",
    "zh-TW": "下載寫入磁碟失敗：{detail}",
    en: "Failed to flush the download to disk: {detail}",
  },
  "runtime.node.checksum_entry_missing": {
    "zh-CN": "官方校验文件缺少 {file}",
    "zh-TW": "官方校驗檔缺少 {file}",
    en: "The official checksum file has no entry for {file}.",
  },
  "runtime.node.checksum_mismatch": {
    "zh-CN": "Node 归档的 SHA-256 校验失败",
    "zh-TW": "Node 壓縮檔的 SHA-256 校驗失敗",
    en: "The Node archive failed its SHA-256 checksum.",
  },
  "runtime.node.version_mismatch": {
    "zh-CN": "Node 版本不符：期望 {expected}，实际 {actual}",
    "zh-TW": "Node 版本不符：預期 {expected}，實際 {actual}",
    en: "Node version mismatch: expected {expected}, got {actual}",
  },
  "runtime.node.fixed_version": {
    "zh-CN": "Node 运行时只能安装内置固定版本 {version}",
    "zh-TW": "Node 運行時只能安裝內建固定版本 {version}",
    en: "The Node runtime only accepts the bundled fixed version {version}.",
  },
  "runtime.node.exe_missing": {
    "zh-CN": "Node 运行时缺少 node.exe",
    "zh-TW": "Node 運行時缺少 node.exe",
    en: "The Node runtime is missing node.exe.",
  },
  "runtime.node.npm_missing": {
    "zh-CN": "Node 运行时缺少 npm.cmd（安装 Pi/DSH 需要托管 npm）",
    "zh-TW": "Node 運行時缺少 npm.cmd（安裝 Pi/DSH 需要託管 npm）",
    en: "The Node runtime is missing npm.cmd (the managed npm is required to install Pi/DSH).",
  },
  "runtime.node.verify_failed": {
    "zh-CN": "Node 运行时启动验证失败",
    "zh-TW": "Node 運行時啟動驗證失敗",
    en: "The Node runtime failed its startup verification.",
  },
  "runtime.node.archive_open_failed": {
    "zh-CN": "无法打开 Node 归档：{detail}",
    "zh-TW": "無法開啟 Node 壓縮檔：{detail}",
    en: "Could not open the Node archive: {detail}",
  },
  "runtime.node.archive_invalid": {
    "zh-CN": "Node 归档无效：{detail}",
    "zh-TW": "Node 壓縮檔無效：{detail}",
    en: "The Node archive is invalid: {detail}",
  },
  "runtime.node.archive_read_failed": {
    "zh-CN": "Node 归档读取失败：{detail}",
    "zh-TW": "Node 壓縮檔讀取失敗：{detail}",
    en: "Failed to read the Node archive: {detail}",
  },
  "runtime.node.archive_unsafe_path": {
    "zh-CN": "Node 归档包含非法路径",
    "zh-TW": "Node 壓縮檔包含非法路徑",
    en: "The Node archive contains an unsafe path.",
  },
  "dsh.repair.official_package": {
    "zh-CN": "内置 DSH 组件不能通过修复移除，请改用“运行时与更新”修复 DSH 本体",
    "zh-TW": "內建 DSH 元件不能透過修復移除，請改用「運行時與更新」修復 DSH 本體",
    en: "Built-in DSH components cannot be removed by repair. Use “Runtime & Updates” to repair DSH itself.",
  },
  "dsh.repair.running": {
    "zh-CN": "DSH 正在运行，请先停止 DSH 再执行修复",
    "zh-TW": "DSH 正在運行，請先停止 DSH 再執行修復",
    en: "DSH is running. Stop DSH before repairing.",
  },
  "dsh.profile.missing": {
    "zh-CN": "DSH web 配置目录不存在",
    "zh-TW": "DSH web 設定目錄不存在",
    en: "The DSH web profile directory does not exist.",
  },
  "dsh.repair.not_in_profile": {
    "zh-CN": "插件 {package} 不在 web profile 的依赖或插件清单中",
    "zh-TW": "外掛 {package} 不在 web profile 的相依或外掛清單中",
    en: "Plugin {package} is not listed in the web profile dependencies or bundles.",
  },
  "dsh.start.failed": {
    "zh-CN": "DSH 启动失败（{error}）：{detail}",
    "zh-TW": "DSH 啟動失敗（{error}）：{detail}",
    en: "DSH failed to start ({error}): {detail}",
  },
};
