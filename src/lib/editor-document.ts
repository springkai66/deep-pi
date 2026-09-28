import { Compartment, EditorState } from "@codemirror/state";
import { history } from "@codemirror/commands";
import { t } from "./i18n.svelte";

export const editorViewConfig = new Compartment();
export const editorLanguageConfig = new Compartment();
export const editorReadOnlyConfig = new Compartment();

export type NewlineMode = "mixed" | "crlf" | "cr" | "lf";

export function newlineMode(content: string): NewlineMode {
  const endings = new Set(content.match(/\r\n|\r|\n/g) ?? []);
  return endings.size > 1 ? "mixed" : endings.has("\r\n") ? "crlf" : endings.has("\r") ? "cr" : "lf";
}

/** 换行模式的显示文案；逻辑判断请用 `newlineMode` 的稳定 id。 */
export function newlineLabel(content: string): string {
  const mode = newlineMode(content);
  if (mode === "mixed") return t("混合");
  return mode === "crlf" ? "CRLF" : mode === "cr" ? "CR" : "LF";
}

export function createDocumentState(content: string): EditorState {
  const mode = newlineMode(content);
  // Mixed files retain CR as an explicit character instead of silently normalizing it away.
  const separator = mode === "crlf" ? "\r\n" : mode === "cr" ? "\r" : "\n";
  return EditorState.create({ doc: content, extensions: [
    EditorState.lineSeparator.of(separator), history(),
    editorViewConfig.of([]), editorLanguageConfig.of([]), editorReadOnlyConfig.of([]),
  ] });
}
