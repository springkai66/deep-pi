/**
 * 应用级浮动提示（toast）。
 *
 * 用于「操作失败」这类**不该打断用户**的反馈：在窗口上方居中浮现一条贴着文字
 * 大小的提示，几秒后自动消失，也可以点关闭。确认类对话框（删除、覆盖等）仍然
 * 走 AppDialog，不受这里影响。
 */
export type NoticeTone = "error" | "success" | "info";

export interface AppNotice {
  id: number;
  message: string;
  tone: NoticeTone;
}

/** 同屏最多保留的提示条数：超出的从最旧的开始丢弃。 */
export const NOTICE_LIMIT = 4;
/** 默认停留时长（毫秒）；<= 0 表示不自动消失（测试用）。 */
export const NOTICE_TTL = 4200;

class NoticeStore {
  items = $state<AppNotice[]>([]);
  #nextId = 0;
  #timers = new Map<number, ReturnType<typeof setTimeout>>();

  /// 推送一条提示；与现存提示文案、语气都相同时只续期，不重复堆叠。
  push(message: string, tone: NoticeTone = "error", ttl = NOTICE_TTL): number {
    const text = message.trim();
    if (text === "") return 0;
    const existing = this.items.find((item) => item.message === text && item.tone === tone);
    if (existing) {
      this.#schedule(existing.id, ttl);
      return existing.id;
    }
    const id = ++this.#nextId;
    this.items = [...this.items, { id, message: text, tone }].slice(-NOTICE_LIMIT);
    this.#schedule(id, ttl);
    return id;
  }

  dismiss(id: number) {
    this.#clearTimer(id);
    this.items = this.items.filter((item) => item.id !== id);
  }

  clear() {
    for (const timer of this.#timers.values()) clearTimeout(timer);
    this.#timers.clear();
    this.items = [];
  }

  #schedule(id: number, ttl: number) {
    this.#clearTimer(id);
    if (!Number.isFinite(ttl) || ttl <= 0) return;
    this.#timers.set(
      id,
      setTimeout(() => {
        this.#timers.delete(id);
        this.items = this.items.filter((item) => item.id !== id);
      }, ttl),
    );
  }

  #clearTimer(id: number) {
    const timer = this.#timers.get(id);
    if (timer !== undefined) {
      clearTimeout(timer);
      this.#timers.delete(id);
    }
  }
}

export const notices = new NoticeStore();
