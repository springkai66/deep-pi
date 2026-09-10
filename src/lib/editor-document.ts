import { Compartment, EditorState } from "@codemirror/state";
import { history } from "@codemirror/commands";

export const editorViewConfig = new Compartment();
export const editorLanguageConfig = new Compartment();
export const editorReadOnlyConfig = new Compartment();

export function newlineMode(content: string): string {
  const endings = new Set(content.match(/\r\n|\r|\n/g) ?? []);
  return endings.size > 1 ? "混合" : endings.has("\r\n") ? "CRLF" : endings.has("\r") ? "CR" : "LF";
}

export function createDocumentState(content: string): EditorState {
  const mode = newlineMode(content);
  // Mixed files retain CR as an explicit character instead of silently normalizing it away.
  const separator = mode === "CRLF" ? "\r\n" : mode === "CR" ? "\r" : "\n";
  return EditorState.create({ doc: content, extensions: [
    EditorState.lineSeparator.of(separator), history(),
    editorViewConfig.of([]), editorLanguageConfig.of([]), editorReadOnlyConfig.of([]),
  ] });
}
