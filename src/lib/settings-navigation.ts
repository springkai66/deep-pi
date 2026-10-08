export const SETTINGS_CATEGORIES = [
  { id: "general", label: "通用", group: "应用", keywords: "界面语言 关闭窗口 同时运行任务数 默认 Shell 任务完成通知 显示桌宠 桌宠置顶显示 任务环半径 桌宠形象 外部编辑器 DeepPi" },
  { id: "appearance", label: "外观", group: "应用", keywords: "主题 导入主题 导出当前主题 颜色模式 应用程序字体 应用字体大小 会话窗口字体 会话字体大小 代码字体 对话内容显示 磨玻璃透明程度 玻璃透明度 透明程度" },
  { id: "pi", label: "Pi 服务", group: "Pi Coding Agent", subgroup: "服务与模型", keywords: "组件 更新 卸载 Node.js Codemode 默认启动 工具描述预算 内联预算 模式" },
  { id: "models", label: "模型设置", group: "Pi Coding Agent", subgroup: "服务与模型", keywords: "Provider API Key 订阅 Codex" },
  { id: "extensions", label: "Pi 资源包", group: "Pi Coding Agent", subgroup: "扩展与能力", keywords: "插件 扩展 技能 主题 提示模板 市场 安装 包目录 作者" },
  { id: "mcp", label: "MCP 服务", group: "Pi Coding Agent", subgroup: "扩展与能力", keywords: "服务器 市场 安装" },
  { id: "skills", label: "Skills 技能", group: "Pi Coding Agent", subgroup: "扩展与能力", keywords: "技能 市场 安装" },
  { id: "workflows", label: "工作流", group: "Pi Coding Agent", subgroup: "扩展与能力", keywords: "流程 市场 安装" },
  { id: "dsh", label: "DSH 服务", group: "DSH (DeepSeek Harness)", keywords: "组件 检测 修复 更新" },
  { id: "network", label: "网络代理", group: "系统", keywords: "代理模式 代理地址 例外地址 连通性测试 PAC" },
  { id: "advanced", label: "高级与诊断", group: "系统", keywords: "运行环境 关于 DeepPi" },
] as const;

export type SettingsCategory = typeof SETTINGS_CATEGORIES[number]["id"];

export interface SettingsGroup {
  label: string;
  categories: { id: SettingsCategory; label: string; subgroup?: string }[];
}

/** 按首次出现顺序把扁平分类聚合成导航分组。 */
export function settingsGroups(
  categories: ReadonlyArray<{ id: SettingsCategory; label: string; group: string; subgroup?: string }> = SETTINGS_CATEGORIES,
): SettingsGroup[] {
  const groups: SettingsGroup[] = [];
  for (const category of categories) {
    let group = groups.find((candidate) => candidate.label === category.group);
    if (!group) {
      group = { label: category.group, categories: [] };
      groups.push(group);
    }
    group.categories.push({ id: category.id, label: category.label, subgroup: category.subgroup });
  }
  return groups;
}

export function filterSettingsCategories(query: string, translate: (text: string) => string = (text) => text): SettingsCategory[] {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return SETTINGS_CATEGORIES.map((item) => item.id);
  return SETTINGS_CATEGORIES.filter((item) =>
    [item.label, item.group, "subgroup" in item ? item.subgroup : "", item.keywords]
      .some((text) => text && (text.toLocaleLowerCase().includes(needle)
        || translate(text).toLocaleLowerCase().includes(needle)
        || text.split(/\s+/).some((word) => translate(word).toLocaleLowerCase().includes(needle)))),
  ).map((item) => item.id);
}
export function nextSettingsCategory(current: SettingsCategory, key: string, visible: readonly SettingsCategory[] = SETTINGS_CATEGORIES.map((item) => item.id)): SettingsCategory | null {
  if (!visible.length) return null;
  const index = visible.indexOf(current);
  if (key === "Home") return visible[0];
  if (key === "End") return visible[visible.length - 1];
  const direction = key === "ArrowDown" || key === "ArrowRight" ? 1 : key === "ArrowUp" || key === "ArrowLeft" ? -1 : 0;
  return direction ? visible[(index + direction + visible.length) % visible.length] : null;
}

export function parseTaskLimit(value: string): number | null {
  if (!/^\d+$/.test(value)) return null;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed >= 1 && parsed <= 16 ? parsed : null;
}
