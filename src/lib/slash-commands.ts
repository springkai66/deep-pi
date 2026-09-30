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
  /** 是否有 DeepPi 图形界面处理入口。 */
  available: boolean;
}

/** Pi 0.99.1 内置命令（对齐托管包的 core/slash-commands.js）。 */
export const BUILTIN_SLASH_COMMANDS: SlashCommand[] = [
  { name: "compact", description: "手动压缩会话上下文", source: "builtin", available: true },
  { name: "model", description: "选择本会话使用的模型", argumentHint: "[provider/model]", source: "builtin", available: true },
  { name: "thinking", description: "设置推理强度", argumentHint: "<level>", source: "builtin", available: true },
  { name: "name", description: "设置会话名称", argumentHint: "<title>", source: "builtin", available: true },
  { name: "copy", description: "复制最后一条回复", source: "builtin", available: true },
  { name: "session", description: "显示会话信息与用量", source: "builtin", available: true },
  { name: "export", description: "导出会话为 HTML", source: "builtin", available: true },
  { name: "tree", description: "打开对话历史跳转", source: "builtin", available: true },
  { name: "new", description: "开始新会话", source: "builtin", available: true },
  { name: "settings", description: "打开设置", source: "builtin", available: true },
  { name: "resume", description: "恢复其他会话", source: "builtin", available: true },
  { name: "scoped-models", description: "启用/禁用轮换模型", source: "builtin", available: true },
  { name: "import", description: "从 JSONL 导入并恢复会话", source: "builtin", available: true },
  { name: "share", description: "将会话分享为私密 GitHub Gist", source: "builtin", available: true },
  { name: "changelog", description: "查看更新日志", source: "builtin", available: true },
  { name: "hotkeys", description: "查看快捷键", source: "builtin", available: true },
  { name: "fork", description: "从历史消息创建会话分支", source: "builtin", available: true },
  { name: "clone", description: "复制当前会话", source: "builtin", available: true },
  { name: "trust", description: "保存项目信任决定", source: "builtin", available: true },
  { name: "login", description: "配置 Provider 登录", source: "builtin", available: true },
  { name: "logout", description: "移除 Provider 登录", source: "builtin", available: true },
  { name: "reload", description: "重新加载扩展/技能/提示词", source: "builtin", available: true },
  { name: "quit", description: "退出应用", source: "builtin", available: true },
  { name: "bug", description: "报告 Pi 的问题", source: "builtin", available: false },
];
export const HOST_SLASH_COMMANDS = new Set([
  "new", "settings", "login", "logout", "reload", "quit", "resume", "scoped-models",
  "changelog", "hotkeys", "import", "share", "trust",
]);
export function slashCommandRoute(name: string, piCommands: PiCommand[]): "host" | "rpc" | "prompt" | "blocked" | "unknown" {
  const builtin = BUILTIN_SLASH_COMMANDS.find((command) => command.name === name);
  if (builtin) {
    if (!builtin.available) return "blocked";
    return HOST_SLASH_COMMANDS.has(name) ? "host" : "rpc";
  }
  return piCommands.some((command) => command.name === name) ? "prompt" : "unknown";
}

export async function runHostSlash(action: () => Promise<boolean>, onError: (error: unknown) => void): Promise<"handled" | "blocked"> {
  try {
    return await action() ? "handled" : "blocked";
  } catch (error) {
    onError(error);
    return "blocked";
  }
}

export function supportsProviderLogin(providerId: string, providers: { id: string; oauth: boolean; apiKey?: boolean }[]): boolean {
  return providers.some((provider) => (provider.oauth || provider.apiKey) && provider.id === providerId);
}

export async function syncSessionRebound(rebound: () => Promise<void>, reconnect: () => void, onError: (error: unknown) => void): Promise<boolean> {
  try {
    await rebound();
    return true;
  } catch (error) {
    onError(error);
    return false;
  } finally {
    reconnect();
  }
}

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
