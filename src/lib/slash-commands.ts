import { t } from "./i18n.svelte";

/** pi `get_commands` 返回的原始命令（扩展、提示词模板、技能）。 */
export interface PiCommand {
  name: string;
  description: string;
  source: string;
}

/** 输入框 `/` 建议列表条目；description 为简体中文原文，渲染时经 t() 翻译。 */
export interface SlashCommand {
  name: string;
  description: string;
  argumentHint?: string;
  source: "builtin" | "extension" | "prompt" | "skill";
  /** DeepPi 能直接执行；false 表示仅 pi 终端兼容模式可用。 */
  available: boolean;
}

/**
 * pi 内置命令（对齐 pi 的 BUILTIN_SLASH_COMMANDS）：
 * available 的命令由 DeepPi 映射到 RPC/界面动作；其余标注仅终端模式可用。
 */
export const BUILTIN_SLASH_COMMANDS: SlashCommand[] = [
  { name: "compact", description: "手动压缩会话上下文", source: "builtin", available: true },
  { name: "model", description: "选择本会话使用的模型", argumentHint: "[provider/model]", source: "builtin", available: true },
  { name: "thinking", description: "设置推理强度", argumentHint: "<level>", source: "builtin", available: true },
  { name: "name", description: "设置会话名称", argumentHint: "<title>", source: "builtin", available: true },
  { name: "copy", description: "复制最后一条回复", source: "builtin", available: true },
  { name: "session", description: "显示会话信息与用量", source: "builtin", available: true },
  { name: "export", description: "导出会话为 HTML", source: "builtin", available: true },
  { name: "tree", description: "打开对话历史跳转", source: "builtin", available: true },
  { name: "new", description: "开始新会话（DeepPi 请用「新建任务」按钮）", source: "builtin", available: false },
  { name: "settings", description: "打开设置（DeepPi 请用左侧设置按钮）", source: "builtin", available: false },
  { name: "resume", description: "恢复其他会话（仅终端兼容模式可用）", source: "builtin", available: false },
  { name: "scoped-models", description: "启用/禁用轮换模型（仅终端兼容模式可用）", source: "builtin", available: false },
  { name: "import", description: "从 JSONL 导入并恢复会话（仅终端兼容模式可用）", source: "builtin", available: false },
  { name: "share", description: "将会话分享为私密 GitHub Gist（仅终端兼容模式可用）", source: "builtin", available: false },
  { name: "changelog", description: "查看更新日志（仅终端兼容模式可用）", source: "builtin", available: false },
  { name: "hotkeys", description: "查看快捷键（仅终端兼容模式可用）", source: "builtin", available: false },
  { name: "fork", description: "从历史消息创建会话分支（仅终端兼容模式可用）", source: "builtin", available: false },
  { name: "clone", description: "复制当前会话（仅终端兼容模式可用）", source: "builtin", available: false },
  { name: "trust", description: "保存项目信任决定（仅终端兼容模式可用）", source: "builtin", available: false },
  { name: "login", description: "配置 Provider 登录（仅终端兼容模式可用）", source: "builtin", available: false },
  { name: "logout", description: "移除 Provider 登录（仅终端兼容模式可用）", source: "builtin", available: false },
  { name: "reload", description: "重新加载扩展/技能/提示词（仅终端兼容模式可用）", source: "builtin", available: false },
  { name: "quit", description: "退出应用（仅终端兼容模式可用）", source: "builtin", available: false },
];

/** 解析整条草稿是否为 `/name args` 形式；纯文本或空返回 null。 */
export function parseSlashCommand(text: string): { name: string; args: string } | null {
  const match = /^\/([A-Za-z0-9_:.-]+)(?:\s+([\s\S]*))?$/.exec(text.trim());
  if (!match) return null;
  return { name: match[1], args: (match[2] ?? "").trim() };
}

function normalizePiCommand(command: PiCommand): SlashCommand | null {
  const name = typeof command?.name === "string" ? command.name.trim() : "";
  if (!name) return null;
  const source = command.source === "prompt" || command.source === "skill" || command.source === "extension"
    ? command.source
    : "extension";
  return {
    name,
    description: typeof command.description === "string" ? command.description : "",
    source,
    available: true,
  };
}

/**
 * 合并内置命令与 pi 命令并按输入过滤：前缀命中优先，其次名称/描述包含。
 * 内置命令排在前、pi 命令去重；空查询返回列表头部。
 */
export function filterSlashCommands(query: string, piCommands: PiCommand[], limit = 12): SlashCommand[] {
  const needle = query.trim().replace(/^\//, "").toLowerCase();
  const seen = new Set<string>();
  const merged: SlashCommand[] = [];
  for (const command of BUILTIN_SLASH_COMMANDS) {
    seen.add(command.name);
    merged.push(command);
  }
  for (const command of piCommands) {
    const normalized = normalizePiCommand(command);
    if (!normalized || seen.has(normalized.name)) continue;
    seen.add(normalized.name);
    merged.push(normalized);
  }
  if (!needle) return merged.slice(0, limit);
  const matches = merged.filter((command) => command.name.toLowerCase().includes(needle)
    || command.description.toLowerCase().includes(needle));
  matches.sort((left, right) => {
    const leftPrefix = left.name.toLowerCase().startsWith(needle) ? 0 : 1;
    const rightPrefix = right.name.toLowerCase().startsWith(needle) ? 0 : 1;
    return leftPrefix - rightPrefix;
  });
  return matches.slice(0, limit);
}

/** 建议列表右侧来源标签。 */
export function slashSourceLabel(command: SlashCommand): string {
  if (!command.available) return t("终端");
  if (command.source === "extension") return t("扩展");
  if (command.source === "prompt") return t("模板");
  if (command.source === "skill") return t("技能");
  return t("内置");
}
