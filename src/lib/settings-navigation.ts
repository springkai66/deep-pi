export const SETTINGS_CATEGORIES = [
  { id: "general", label: "通用", group: "应用" },
  { id: "appearance", label: "外观", group: "应用" },
  { id: "models", label: "模型设置", group: "Pi Coding Agent" },
  { id: "extensions", label: "Pi 扩展", group: "Pi Coding Agent" },
  { id: "mcp", label: "MCP 服务", group: "Pi Coding Agent" },
  { id: "skills", label: "Skills 技能", group: "Pi Coding Agent" },
  { id: "workflows", label: "工作流", group: "Pi Coding Agent" },
  { id: "dsh", label: "DSH 服务", group: "DSH (DeepSeek Harness)" },
  { id: "runtime", label: "运行时与更新", group: "系统" },
  { id: "network", label: "网络代理", group: "系统" },
  { id: "advanced", label: "高级与诊断", group: "系统" },
] as const;

export type SettingsCategory = typeof SETTINGS_CATEGORIES[number]["id"];

export interface SettingsGroup {
  label: string;
  categories: { id: SettingsCategory; label: string }[];
}

/** 按首次出现顺序把扁平分类聚合成导航分组。 */
export function settingsGroups(
  categories: ReadonlyArray<{ id: SettingsCategory; label: string; group: string }> = SETTINGS_CATEGORIES,
): SettingsGroup[] {
  const groups: SettingsGroup[] = [];
  for (const category of categories) {
    let group = groups.find((candidate) => candidate.label === category.group);
    if (!group) {
      group = { label: category.group, categories: [] };
      groups.push(group);
    }
    group.categories.push({ id: category.id, label: category.label });
  }
  return groups;
}

export function nextSettingsCategory(current: SettingsCategory, key: string): SettingsCategory | null {
  const index = SETTINGS_CATEGORIES.findIndex((category) => category.id === current);
  if (key === "Home") return SETTINGS_CATEGORIES[0].id;
  if (key === "End") return SETTINGS_CATEGORIES[SETTINGS_CATEGORIES.length - 1].id;
  const direction = key === "ArrowDown" || key === "ArrowRight" ? 1 : key === "ArrowUp" || key === "ArrowLeft" ? -1 : 0;
  return direction ? SETTINGS_CATEGORIES[(index + direction + SETTINGS_CATEGORIES.length) % SETTINGS_CATEGORIES.length].id : null;
}

export function parseTaskLimit(value: string): number | null {
  if (!/^\d+$/.test(value)) return null;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed >= 1 && parsed <= 16 ? parsed : null;
}
