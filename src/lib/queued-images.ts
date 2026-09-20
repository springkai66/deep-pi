/**
 * 排队中提示词的图片记忆：pi 的 `clear_queue` 与队列事件只携带文本，
 * 图片由 DeepPi 本地记住，供「待发送」条目编辑/移动/撤回以及队列重建时恢复。
 * 泛型 `T` 为图片负载类型（组件里是 ChatImageAttachment）。
 */
export interface QueuedImageEntry<T> {
  mode: "steer" | "followUp";
  text: string;
  images: T[];
}

/** 是否存在匹配条目的图片记忆（用于待发送列表的“含图片”标记）。 */
export function hasQueuedImages<T>(entries: QueuedImageEntry<T>[], mode: "steer" | "followUp", text: string): boolean {
  return entries.some((entry) => entry.mode === mode && entry.text === text);
}

/** 取出并移除匹配条目的图片；同文案多条时按顺序消费第一条。 */
export function takeQueuedImages<T>(
  entries: QueuedImageEntry<T>[],
  mode: "steer" | "followUp",
  text: string,
): { entries: QueuedImageEntry<T>[]; images: T[] } {
  const index = entries.findIndex((entry) => entry.mode === mode && entry.text === text);
  if (index < 0) return { entries, images: [] };
  const next = [...entries];
  const [found] = next.splice(index, 1);
  return { entries: next, images: found.images };
}

/** 把首条匹配条目的记忆改挂到目标模式（移动队列时图片随重发保留；同文案多条按顺序只改第一条）。 */
export function rekeyQueuedImages<T>(
  entries: QueuedImageEntry<T>[],
  from: "steer" | "followUp",
  to: "steer" | "followUp",
  text: string,
): QueuedImageEntry<T>[] {
  const index = entries.findIndex((entry) => entry.mode === from && entry.text === text);
  if (index < 0) return entries;
  return entries.map((entry, position) => position === index ? { ...entry, mode: to } : entry);
}
