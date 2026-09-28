import type { GitDiffArea } from "./git-status";

export interface GitDiffSelection { projectId: string; path: string; area: GitDiffArea }
export type ConflictSide = "base" | "ours" | "theirs";
export interface GitDiff {
  path: string;
  area: GitDiffArea;
  patch: string;
  format: "text" | "binary" | "unsupportedEncoding";
  truncated: boolean;
  sourceOutsideProject: boolean;
}

export const DIFF_AREA_LABELS: Record<GitDiffArea, string> = {
  staged: "已暂存差异", unstaged: "未暂存差异", untracked: "新文件", conflict: "冲突差异",
};

export interface DiffRow {
  kind: "meta" | "hunk" | "context" | "add" | "delete" | "note";
  text: string;
  oldLine: number | null;
  newLine: number | null;
}

export function diffScrollPosition(
  event: { key: string; ctrlKey?: boolean; metaKey?: boolean; altKey?: boolean; shiftKey?: boolean; isComposing?: boolean },
  viewport: { top: number; left: number; height: number; scrollHeight: number },
) {
  if (event.isComposing || event.altKey || event.shiftKey
    || ((event.ctrlKey || event.metaKey) && !["Home", "End"].includes(event.key))) return null;
  let { top, left } = viewport;
  switch (event.key) {
    case "Home": top = 0; break;
    case "End": top = viewport.scrollHeight - viewport.height; break;
    case "PageUp": top -= viewport.height; break;
    case "PageDown": top += viewport.height; break;
    case "ArrowUp": top -= 22; break;
    case "ArrowDown": top += 22; break;
    case "ArrowLeft": left -= 40; break;
    case "ArrowRight": left += 40; break;
    default: return null;
  }
  return { top: Math.max(0, Math.min(top, viewport.scrollHeight - viewport.height)), left: Math.max(0, left) };
}

export function parseUnifiedDiff(patch: string): DiffRow[] {
  const lines = patch.split("\n");
  if (lines.at(-1) === "") lines.pop();
  let oldLine = 0, newLine = 0, oldLeft = 0, newLeft = 0;
  return lines.map((text): DiffRow => {
    const hunk = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(text);
    if (hunk) {
      oldLine = Number(hunk[1]); newLine = Number(hunk[3]);
      oldLeft = Number(hunk[2] ?? 1); newLeft = Number(hunk[4] ?? 1);
      return { kind: "hunk", text, oldLine: null, newLine: null };
    }
    if (text.startsWith("\\ ")) return { kind: "note", text, oldLine: null, newLine: null };
    if (oldLeft || newLeft) {
      if (text.startsWith("+") && newLeft) {
        newLeft--;
        return { kind: "add", text: text.slice(1), oldLine: null, newLine: newLine++ };
      }
      if (text.startsWith("-") && oldLeft) {
        oldLeft--;
        return { kind: "delete", text: text.slice(1), oldLine: oldLine++, newLine: null };
      }
      if (text.startsWith(" ") && oldLeft && newLeft) {
        oldLeft--; newLeft--;
        return { kind: "context", text: text.slice(1), oldLine: oldLine++, newLine: newLine++ };
      }
    }
    oldLeft = 0; newLeft = 0;
    return { kind: "meta", text, oldLine: null, newLine: null };
  });
}
