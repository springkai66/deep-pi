import { LanguageDescription } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { classHighlighter, highlightTree } from "@lezer/highlight";
function escapeCode(source: string) {
  return source.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

export async function highlightMessageCode(source: string, requested: string): Promise<string> {
  if (source.length > 64 * 1024) return escapeCode(source);
  const language = LanguageDescription.matchLanguageName(languages, requested, false);
  if (!language) return escapeCode(source);
  try {
    const support = await language.load();
    const tree = support.language.parser.parse(source);
    const output: string[] = [];
    let offset = 0;
    highlightTree(tree, classHighlighter, (from, to, classes) => {
      output.push(escapeCode(source.slice(offset, from)));
      // Class names come only from Lezer's fixed classHighlighter, never Markdown.
      output.push(`<span class="${classes}">${escapeCode(source.slice(from, to))}</span>`);
      offset = to;
    });
    output.push(escapeCode(source.slice(offset)));
    return output.join("");
  } catch { return escapeCode(source); }
}
