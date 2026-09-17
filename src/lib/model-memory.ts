/**
 * 按项目记住上次手动选择的模型与推理强度。
 *
 * 持久化在任务数据库（project_model_prefs 表，键为 project id；项目是软删除，
 * 重新添加同一目录会复用原 id，选择随之沿用）。这里只放「应用/跳过」的纯决策：
 * ChatPane 在新会话连接完成后调用，恢复已有会话时不做任何覆盖。
 */

export interface SavedModelChoice {
  provider: string;
  modelId: string;
  /** null 表示该项目从未记录过推理强度：只应用模型，不强行设置档位。 */
  thinkingLevel: string | null;
}

export interface ModelOption {
  id: string;
  provider: string;
  name: string;
}

/** 新会话判定：没有任何历史消息且当前不在流式输出中。恢复的会话历史非空，一律不覆盖。 */
export function shouldApplySavedChoice(historyCount: number, busy: boolean): boolean {
  return historyCount === 0 && !busy;
}

/** 记住的模型必须仍出现在当前可用列表中才应用；找不到时静默跳过（不报错、不改选择）。 */
export function findSavedModel(saved: SavedModelChoice, models: ModelOption[]): ModelOption | null {
  return models.find((model) => model.provider === saved.provider && model.id === saved.modelId) ?? null;
}

/** 记住的推理强度必须属于当前模型的可用档位；从未记录（null/空）或已失效时跳过。 */
export function validSavedThinkingLevel(level: string | null, levels: string[]): string | null {
  if (typeof level !== "string" || !level) return null;
  return levels.includes(level) ? level : null;
}

/** 选择器的 "provider/modelId" 值拆回两端；modelId 本身可以包含 “/”（如 OpenRouter）。 */
export function parseModelKey(key: string): { provider: string; modelId: string } | null {
  const index = key.indexOf("/");
  if (index <= 0 || index === key.length - 1) return null;
  return { provider: key.slice(0, index), modelId: key.slice(index + 1) };
}
