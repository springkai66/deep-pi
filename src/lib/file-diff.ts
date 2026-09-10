import { EditorState, type EditorStateConfig } from "@codemirror/state";
import { EditorView, drawSelection, lineNumbers, highlightSpecialChars, keymap } from "@codemirror/view";
import type { DirectMergeConfig } from "@codemirror/merge";
import { newlineMode } from "./editor-document";
import { shortcutKeymap } from "./shortcuts";

const normalized = (content: string) => content.replace(/\r\n?/g, "\n");

export function diffMetadata(source: string, draft: string) {
  return {
    equal: source === draft,
    newlineOnly: source !== draft && normalized(source) === normalized(draft),
    sourceNewline: newlineMode(source),
    draftNewline: newlineMode(draft),
    sourceBom: source.startsWith("\uFEFF"),
    draftBom: draft.startsWith("\uFEFF"),
  };
}

function readOnlySide(content: string, label: string, navigate?: (direction: 1 | -1) => void): EditorStateConfig {
  return {
    doc: normalized(content),
    extensions: [
      EditorState.readOnly.of(true), EditorView.editable.of(false),
      EditorState.transactionFilter.of((transaction) => transaction.docChanged ? [] : transaction),
      lineNumbers(), drawSelection(), highlightSpecialChars(),
      keymap.of(navigate ? [
        { key: shortcutKeymap("nextDiff"), run: (view) => { if (view.composing) return false; navigate(1); return true; } },
        { key: shortcutKeymap("previousDiff"), run: (view) => { if (view.composing) return false; navigate(-1); return true; } },
      ] : []),
      EditorView.contentAttributes.of({ "aria-label": label, "aria-readonly": "true", tabindex: "0" }),
      EditorView.theme({
        "&": { color: "var(--text)", backgroundColor: "var(--page-bg)" },
        ".cm-scroller": { fontFamily: "var(--code-font)", fontSize: "13px", overflowX: "auto" },
        ".cm-content": { padding: "12px 0" },
        ".cm-gutters": { backgroundColor: "var(--surface)", color: "var(--text-muted)", borderRight: "1px solid var(--border)" },
        "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": { backgroundColor: "var(--surface-hover)" },
        "&.cm-merge-a .cm-changedLine": { backgroundColor: "var(--diff-delete-line)" },
        "&.cm-merge-b .cm-changedLine": { backgroundColor: "var(--diff-add-line)" },
        "&.cm-merge-a .cm-changedText": { backgroundColor: "var(--diff-delete-text)" },
        "&.cm-merge-b .cm-changedText": { backgroundColor: "var(--diff-add-text)" },
        "&.cm-merge-a .cm-changedLineGutter": { backgroundColor: "var(--diff-delete-mark)" },
        "&.cm-merge-b .cm-changedLineGutter": { backgroundColor: "var(--diff-add-mark)" },
      }),
    ],
  };
}

export function createDiffConfig(source: string, draft: string, navigate?: (direction: 1 | -1) => void): DirectMergeConfig {
  return {
    a: readOnlySide(source, "来源文件内容（只读）", navigate),
    b: readOnlySide(draft, "草稿快照（只读）", navigate),
    highlightChanges: true, gutter: true,
    diffConfig: { scanLimit: 500, timeout: 100 },
  };
}

export function nextDiffIndex(current: number, count: number, direction: 1 | -1): number {
  if (!count) return -1;
  if (current < 0) return direction === 1 ? 0 : count - 1;
  return (current + direction + count) % count;
}
