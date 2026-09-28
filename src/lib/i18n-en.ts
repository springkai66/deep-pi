/**
 * 英文目录入口。
 *
 * 按界面分区拆成分片文件，便于并行维护与分批补齐：
 *  - `i18n-en-shell.ts`：应用外壳（控制柱、顶栏、面包屑、任务侧栏、标签、看板）
 *  - `i18n-en-chat.ts`：对话面板、设置页框架与导航
 *  - `i18n-en-mcp.ts`：MCP 服务 / Skills 技能设置、插件市场
 *  - `i18n-en-files.ts`：文件侧栏、编辑器、预览、恢复副本、Git 面板
 *  - `i18n-en-settings.ts`：模型设置、DSH 服务、运行时与更新、外部编辑器
 *  - `i18n-en-misc.ts`：诊断面板、对话框、消息渲染、终端等杂项
 *
 * 键统一为**简体中文原文**。翻译缺失时 `t()` 会自动回落简体中文。
 */

import { EN_CHAT } from "./i18n-en-chat";
import { EN_FILES } from "./i18n-en-files";
import { EN_MCP } from "./i18n-en-mcp";
import { EN_MISC } from "./i18n-en-misc";
import { EN_SETTINGS } from "./i18n-en-settings";
import { EN_SHELL } from "./i18n-en-shell";

export const EN_MESSAGES: Record<string, string> = {
  ...EN_SHELL,
  ...EN_CHAT,
  ...EN_MCP,
  ...EN_FILES,
  ...EN_SETTINGS,
  ...EN_MISC,
};
