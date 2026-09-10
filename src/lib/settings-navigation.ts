export const SETTINGS_CATEGORIES = [
  { id: "general", label: "通用" },
  { id: "appearance", label: "外观" },
  { id: "models", label: "模型与凭据" },
  { id: "runtime", label: "运行时与更新" },
  { id: "extensions", label: "Pi 扩展" },
  { id: "advanced", label: "高级与诊断" },
] as const;

export type SettingsCategory = typeof SETTINGS_CATEGORIES[number]["id"];

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
