/**
 * 会话中文件修改的差异对比（纯函数）。
 *
 * Pi 的 edit / write 工具参数携带结构化的新旧内容：
 * - edit: { path | file_path, oldText, newText } —— 片段替换；
 * - write: { path, content } —— 整体写入（原内容不在参数里，无法对比）。
 * 这里从工具参数提取变更并计算行级差异，供会话对话渲染差异卡片。
 *
 * 语义约定（诚实标注，不夸大）：
 * - 参数是"意图中的变更"，不代表已落盘（失败调用也会带参数）；
 * - write 的原内容未知，不做"新建"推断，统计只计写入行数；
 * - edit 的行号是本次替换片段内的行号，不是文件绝对行号。
 */

export type FileChangeTool = "edit" | "write";

export interface FileChange {
  path: string;
  tool: FileChangeTool;
  before: string;
  after: string;
}

export type DiffRowKind = "add" | "del" | "ctx" | "gap";

export interface DiffRow {
  kind: DiffRowKind;
  text: string;
  oldNo: number | null;
  newNo: number | null;
  /** gap 行：被折叠的上下文行数。 */
  count?: number;
}

export interface LineDiff {
  rows: DiffRow[];
  added: number;
  removed: number;
  /** 归一化换行后内容一致。 */
  equal: boolean;
  /** 仅换行风格不同（CRLF/LF）。 */
  newlineOnly: boolean;
  /** 超过体积上限，未计算差异。 */
  truncated: boolean;
}

/** 单侧输入上限（字符）：防止超大文件把聊天流拖垮。 */
const MAX_INPUT_CHARS = 200_000;
/** 单侧行数上限。 */
const MAX_LINES = 1500;
/** LCS 矩阵格子上限（中段行数乘积）。 */
const MAX_LCS_CELLS = 1_500_000;
/** 改动簇前后保留的上下文行数。 */
const CONTEXT_LINES = 3;
/** 渲染行数上限（超出用 gap 收口）。 */
const MAX_RENDER_ROWS = 600;

function splitLines(text: string): string[] {
  return text.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
}

function trailingNewline(text: string): boolean {
  return text.endsWith("\n") || text.endsWith("\r");
}

/**
 * 从工具名与参数中提取文件变更；非文件修改工具、参数不完整（如流式
 * 半程）时返回 null。edit 用 oldText/newText，write 用 content。
 */
export function extractFileChange(toolName: string, args: unknown): FileChange | null {
  const name = typeof toolName === "string" ? toolName.trim().toLowerCase() : "";
  if (name !== "edit" && name !== "write") return null;
  const record = args !== null && typeof args === "object" && !Array.isArray(args)
    ? args as Record<string, unknown>
    : {};
  const rawPath = typeof record.path === "string" && record.path.trim()
    ? record.path
    : typeof record.file_path === "string" && record.file_path.trim() ? record.file_path : null;
  if (!rawPath) return null;
  if (name === "edit") {
    if (typeof record.oldText !== "string" || typeof record.newText !== "string") return null;
    return { path: rawPath, tool: "edit", before: record.oldText, after: record.newText };
  }
  if (typeof record.content !== "string") return null;
  return { path: rawPath, tool: "write", before: "", after: record.content };
}

/**
 * 计算行级差异。返回的行号是变更片段内的行号（1 起），不是文件绝对行号；
 * 仅换行风格不同时不产出差异行（newlineOnly 标记，由 UI 提示）。
 */
export function computeLineDiff(before: string, after: string): LineDiff {
  const empty: LineDiff = { rows: [], added: 0, removed: 0, equal: false, newlineOnly: false, truncated: true };
  if (before.length > MAX_INPUT_CHARS || after.length > MAX_INPUT_CHARS) return empty;

  const normalizedBefore = before.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
  const normalizedAfter = after.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
  const newlineOnly = normalizedBefore === normalizedAfter && before !== after;
  if (normalizedBefore === normalizedAfter) {
    return { rows: [], added: 0, removed: 0, equal: true, newlineOnly, truncated: false };
  }

  const beforeLines = splitLines(before);
  const afterLines = splitLines(after);
  if (beforeLines.length > MAX_LINES || afterLines.length > MAX_LINES) return empty;

  // 先裁掉公共前后缀，LCS 只跑中段。
  let prefix = 0;
  while (
    prefix < beforeLines.length && prefix < afterLines.length
    && beforeLines[prefix] === afterLines[prefix]
  ) prefix += 1;
  let suffix = 0;
  while (
    suffix < beforeLines.length - prefix && suffix < afterLines.length - prefix
    && beforeLines[beforeLines.length - 1 - suffix] === afterLines[afterLines.length - 1 - suffix]
  ) suffix += 1;

  const midBefore = beforeLines.slice(prefix, beforeLines.length - suffix);
  const midAfter = afterLines.slice(prefix, afterLines.length - suffix);
  if (midBefore.length * midAfter.length > MAX_LCS_CELLS) return empty;

  // 经典 DP 求 LCS 长度表，再回溯出中段差异行。
  const rows: DiffRow[] = [];
  let added = 0;
  let removed = 0;
  if (midBefore.length === 0 || midAfter.length === 0) {
    for (const text of midBefore) rows.push({ kind: "del", text, oldNo: prefix + 1 + removed, newNo: null });
    for (const text of midAfter) rows.push({ kind: "add", text, oldNo: null, newNo: prefix + 1 + added });
    removed += midBefore.length;
    added += midAfter.length;
  } else {
    const m = midBefore.length;
    const n = midAfter.length;
    const table: Uint32Array[] = Array.from({ length: m + 1 }, () => new Uint32Array(n + 1));
    for (let i = m - 1; i >= 0; i -= 1) {
      for (let j = n - 1; j >= 0; j -= 1) {
        table[i][j] = midBefore[i] === midAfter[j]
          ? table[i + 1][j + 1] + 1
          : Math.max(table[i + 1][j], table[i][j + 1]);
      }
    }
    let i = 0;
    let j = 0;
    let oldNo = prefix + 1;
    let newNo = prefix + 1;
    while (i < m && j < n) {
      if (midBefore[i] === midAfter[j]) {
        rows.push({ kind: "ctx", text: midBefore[i], oldNo, newNo });
        i += 1; j += 1; oldNo += 1; newNo += 1;
      } else if (table[i + 1][j] >= table[i][j + 1]) {
        rows.push({ kind: "del", text: midBefore[i], oldNo, newNo: null });
        removed += 1; i += 1; oldNo += 1;
      } else {
        rows.push({ kind: "add", text: midAfter[j], oldNo: null, newNo });
        added += 1; j += 1; newNo += 1;
      }
    }
    while (i < m) {
      rows.push({ kind: "del", text: midBefore[i], oldNo, newNo: null });
      removed += 1; i += 1; oldNo += 1;
    }
    while (j < n) {
      rows.push({ kind: "add", text: midAfter[j], oldNo: null, newNo });
      added += 1; j += 1; newNo += 1;
    }
  }

  // 公共前后缀以 ctx 行补齐（前缀在头部、后缀在尾部）。
  const headContext: DiffRow[] = [];
  for (let k = 0; k < prefix; k += 1) {
    headContext.push({ kind: "ctx", text: beforeLines[k], oldNo: k + 1, newNo: k + 1 });
  }
  const tailContext: DiffRow[] = [];
  for (let k = 0; k < suffix; k += 1) {
    const oldNo = beforeLines.length - suffix + k + 1;
    tailContext.push({ kind: "ctx", text: beforeLines[beforeLines.length - suffix + k], oldNo, newNo: oldNo });
  }

  const all = [...headContext, ...rows, ...tailContext];
  return {
    rows: collapseContext(all, afterLines.length + beforeLines.length),
    added,
    removed,
    equal: false,
    newlineOnly,
    truncated: false,
  };
}

/** 折叠远离改动的长段上下文：改动簇前后保留 CONTEXT_LINES 行，其余并为 gap。 */
function collapseContext(rows: DiffRow[], fallbackCap: number): DiffRow[] {
  const keep = new Array<boolean>(rows.length).fill(false);
  rows.forEach((row, index) => {
    if (row.kind === "ctx") return;
    for (let d = 0; d <= CONTEXT_LINES; d += 1) {
      if (index - d >= 0) keep[index - d] = true;
      if (index + d < rows.length) keep[index + d] = true;
    }
  });
  const output: DiffRow[] = [];
  let gapCount = 0;
  let gapOld: number | null = null;
  const flushGap = () => {
    if (gapCount > 0) {
      output.push({ kind: "gap", text: "", oldNo: null, newNo: null, count: gapCount });
      gapCount = 0;
    }
  };
  for (let index = 0; index < rows.length; index += 1) {
    const row = rows[index];
    if (row.kind === "ctx" && !keep[index]) {
      gapCount += 1;
      continue;
    }
    flushGap();
    output.push(row);
  }
  flushGap();
  // 渲染上限：超出行数从中间再收一刀。
  if (output.length > MAX_RENDER_ROWS && output.length > 2) {
    const half = Math.floor(MAX_RENDER_ROWS / 2);
    const skipped = output.length - MAX_RENDER_ROWS;
    const head = output.slice(0, half);
    const tail = output.slice(output.length - half);
    const midOld = head.at(-1)?.oldNo ?? null;
    const midNew = head.at(-1)?.newNo ?? null;
    void fallbackCap;
    return [...head, { kind: "gap", text: "", oldNo: midOld, newNo: midNew, count: skipped }, ...tail];
  }
  return output;
}

/** 末尾换行差异提示：某一侧以换行结尾而另一侧没有时返回 true。 */
export function trailingNewlineDiffers(before: string, after: string): boolean {
  return trailingNewline(before) !== trailingNewline(after);
}
