import { t } from "./i18n.svelte";
import { asMessage, record, type Conversation, type RpcMessage } from "./rpc-state";

export interface RpcHistoryModel {
  provider: string;
  id: string;
}

export interface RpcHistoryPage {
  snapshotId: string;
  eventSequence: number;
  start: number;
  end: number;
  total: number;
  messages: RpcMessage[];
  /** 休眠历史专用：会话记录的最后模型；在线历史快照为 null。 */
  model?: RpcHistoryModel | null;
  /** 休眠历史专用：会话记录的最后推理强度；在线历史快照为 null。 */
  thinkingLevel?: string | null;
}

export function validateHistoryPage(value: unknown): RpcHistoryPage {
  const page = record(value);
  if (typeof page.snapshotId !== "string" || !page.snapshotId || page.snapshotId.length > 128
    || ![page.eventSequence, page.start, page.end, page.total].every((item) => typeof item === "number" && Number.isSafeInteger(item) && item >= 0)
    || !Array.isArray(page.messages) || page.messages.length > 100) throw new Error(t("历史分页响应格式无效"));
  const { start, end, total } = page as unknown as RpcHistoryPage;
  if (start > end || end > total || end - start !== page.messages.length) throw new Error(t("历史分页范围无效"));
  const messages = page.messages.map(asMessage);
  if (messages.some((message) => message === null)) throw new Error(t("历史分页包含无效消息"));
  const rawModel = record(page.model);
  const model = typeof rawModel.provider === "string" && rawModel.provider
    && typeof rawModel.id === "string" && rawModel.id
    ? { provider: rawModel.provider, id: rawModel.id }
    : null;
  const thinkingLevel = typeof page.thinkingLevel === "string" && page.thinkingLevel ? page.thinkingLevel : null;
  return { snapshotId: page.snapshotId, eventSequence: page.eventSequence as number,
    start, end, total, messages: messages as RpcMessage[], model, thinkingLevel };
}

export function prependRpcHistory(conversation: Conversation, page: RpcHistoryPage, offset: number): Conversation {
  if (page.end !== offset || page.start >= page.end) throw new Error(t("历史分页已过期或不连续"));
  return { ...conversation, messages: [...page.messages, ...conversation.messages] };
}

export function createRpcHistorySession(
  invoke: <T>(command: string, args: Record<string, unknown>) => Promise<T>,
  taskId: string, runId: string,
) {
  let closed = false;
  let current: RpcHistoryPage | null = null;
  let opening: Promise<RpcHistoryPage | null> | null = null;
  let loadingOlder = false;
  async function release(snapshotId: string) {
    try { await invoke("rpc_history_close", { taskId, runId, snapshotId }); }
    catch { console.warn("无法释放临时历史快照"); }
  }
  function open(): Promise<RpcHistoryPage | null> {
    if (closed) return Promise.resolve(null);
    if (opening) return opening;
    opening = (async () => {
      const response = await invoke<unknown>("rpc_history_open", { taskId, runId });
      let page: RpcHistoryPage;
      try {
        page = validateHistoryPage(response);
        if (page.end !== page.total) throw new Error(t("初始历史分页不是最新快照"));
      } catch (cause) {
        const id = record(response).snapshotId;
        if (typeof id === "string") await release(id);
        throw cause;
      }
      if (closed) { await release(page.snapshotId); return null; }
      current = page;
      return page;
    })();
    return opening;
  }
  async function older(): Promise<RpcHistoryPage | null> {
    if (closed || loadingOlder || !current || current.start === 0) return null;
    const expected = current;
    loadingOlder = true;
    try {
      const response = await invoke<unknown>("rpc_history_page", {
        taskId, runId, snapshotId: expected.snapshotId, before: expected.start,
      });
      if (closed) return null;
      const page = validateHistoryPage(response);
      if (page.snapshotId !== expected.snapshotId || page.eventSequence !== expected.eventSequence
        || page.total !== expected.total || page.end !== expected.start || page.start >= page.end) {
        throw new Error(t("历史分页与当前快照不一致"));
      }
      current = page;
      return page;
    } finally { loadingOlder = false; }
  }
  async function dispose() {
    closed = true;
    const snapshot = current;
    current = null;
    if (snapshot) await release(snapshot.snapshotId);
  }
  return { open, older, dispose };
}

/**
 * 休眠会话历史：进程未启动时直接读 pi 会话文件（rpc_dormant_history，无状态分页）。
 * 与 createRpcHistorySession 同接口（open/older/dispose），可互换使用。
 */
export function createDormantHistorySession(
  invoke: <T>(command: string, args: Record<string, unknown>) => Promise<T>,
  taskId: string,
  limit = 100,
) {
  let closed = false;
  let current: RpcHistoryPage | null = null;
  let opening: Promise<RpcHistoryPage | null> | null = null;
  let loadingOlder = false;
  function open(): Promise<RpcHistoryPage | null> {
    if (closed) return Promise.resolve(null);
    if (opening) return opening;
    opening = (async () => {
      const page = validateHistoryPage(await invoke<unknown>("rpc_dormant_history", { taskId, before: null, limit }));
      if (closed) return null;
      current = page;
      return page;
    })();
    return opening;
  }
  async function older(): Promise<RpcHistoryPage | null> {
    if (closed || loadingOlder || !current || current.start === 0) return null;
    const expected = current;
    loadingOlder = true;
    try {
      const page = validateHistoryPage(await invoke<unknown>("rpc_dormant_history", {
        taskId, before: expected.start, limit,
      }));
      if (closed) return null;
      if (page.snapshotId !== expected.snapshotId || page.total !== expected.total
        || page.end !== expected.start || page.start >= page.end) {
        throw new Error(t("历史分页与当前快照不一致"));
      }
      current = page;
      return page;
    } finally { loadingOlder = false; }
  }
  async function dispose() {
    closed = true;
    current = null;
  }
  return { open, older, dispose };
}
