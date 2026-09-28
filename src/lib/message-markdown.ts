import { Lexer, type Token, type Tokens } from "marked";
import { decode } from "he";

const tags = ["p", "h1", "h2", "h3", "h4", "h5", "h6", "strong", "em", "s",
  "blockquote", "ul", "ol", "li", "table", "thead", "tbody", "tr", "th", "td", "a", "span"] as const;
type MarkdownTag = typeof tags[number];
export type MarkdownNode =
  | { kind: "text"; content: string }
  | { kind: "code"; content: string; language: string; block: boolean }
  | { kind: "break" | "rule" }
  | { kind: "image"; content: string; href: string | null }
  | { kind: "element"; tag: MarkdownTag; children: MarkdownNode[]; href?: string;
      title?: string; start?: number; align?: "left" | "center" | "right" };
export interface ParsedMarkdown { nodes: MarkdownNode[]; plain: boolean }

export function safeMessageLink(value: string): string | null {
  if (!/^https?:\/\//i.test(value) || /[\s\u0000-\u001f\u007f]/u.test(value)) return null;
  try {
    const url = new URL(value);
    if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) return null;
    return url.href;
  } catch { return null; }
}

export async function confirmMessageLink(url: string, ports: {
  current: () => boolean;
  confirm: (destination: string) => Promise<boolean>;
  open: (destination: string) => Promise<void>;
}): Promise<boolean> {
  const safe = safeMessageLink(url);
  if (!safe || !ports.current()) return false;
  if (!await ports.confirm(safe) || !ports.current()) return false;
  await ports.open(safe);
  return true;
}

function nodesFrom(tokens: Token[], budget: { remaining: number }, depth = 0): MarkdownNode[] {
  if (depth > 32) throw new Error("Markdown nesting limit");
  const nodes: MarkdownNode[] = [];
  const element = (tag: MarkdownTag, children: MarkdownNode[]): Extract<MarkdownNode, { kind: "element" }> =>
    ({ kind: "element", tag, children });
  const nested = (values: Token[]) => nodesFrom(values, budget, depth + 1);
  for (const token of tokens) {
    if (--budget.remaining < 0) throw new Error("Markdown node limit");
    if (token.type === "space" || token.type === "def") continue;
    if (token.type === "code" || token.type === "codespan") {
      nodes.push({ kind: "code", content: token.text, language: (token.type === "code" ? token.lang ?? "" : "").trim().split(/\s+/)[0],
        block: token.type === "code" });
    } else if (token.type === "heading") {
      const tag = `h${token.depth}`;
      nodes.push(element(tags.includes(tag as MarkdownTag) ? tag as MarkdownTag : "p", nested(token.tokens ?? [])));
    } else if (token.type === "paragraph" || token.type === "blockquote" || token.type === "strong" || token.type === "em" || token.type === "del") {
      const tag = token.type === "paragraph" ? "p" : token.type === "del" ? "s" : token.type as MarkdownTag;
      nodes.push(element(tag, nested(token.tokens ?? [])));
    } else if (token.type === "list") {
      const list = token as Tokens.List;
      const items = list.items.map((item) => element("li", [
        ...(item.task ? [{ kind: "text" as const, content: item.checked ? "[x] " : "[ ] " }] : []),
        ...nested(item.tokens),
      ]));
      nodes.push({ ...element(list.ordered ? "ol" : "ul", items), start: list.ordered ? Number(list.start) || 1 : undefined });
    } else if (token.type === "table") {
      const table = token as Tokens.Table;
      const row = (cells: Tokens.TableCell[], header: boolean) => element("tr", cells.map((cell, index) => ({
        ...element(header ? "th" : "td", nested(cell.tokens)), align: table.align[index] ?? undefined,
      })));
      nodes.push(element("table", [element("thead", [row(table.header, true)]), element("tbody", table.rows.map((cells) => row(cells, false)))]));
    } else if (token.type === "link") {
      const link = token as Tokens.Link;
      const href = safeMessageLink(link.autolink ? link.href : decode(link.href, { isAttributeValue: true })) ?? undefined;
      nodes.push({ ...element(href ? "a" : "span", nested(link.tokens)), href, title: href });
    } else if (token.type === "image") {
      nodes.push({ kind: "image", content: decode(token.text), href: safeMessageLink(decode(token.href, { isAttributeValue: true })) });
    } else if (token.type === "br") nodes.push({ kind: "break" });
    else if (token.type === "hr") nodes.push({ kind: "rule" });
    else if (token.type === "text" && token.tokens) nodes.push(...nested(token.tokens));
    else if (token.type === "html" || token.type === "escape") {
      nodes.push({ kind: "text", content: token.text });
    } else nodes.push({ kind: "text", content: decode("text" in token ? String(token.text) : token.raw) });
  }
  return nodes;
}

export function parseMessageMarkdown(source: string): ParsedMarkdown {
  const plain = () => ({ nodes: [{ kind: "text" as const, content: source }], plain: true });
  if (source.length > 256 * 1024) return plain();
  try {
    return { nodes: nodesFrom(Lexer.lex(source, { gfm: true, breaks: true }), { remaining: 10_000 }), plain: false };
  } catch { return plain(); }
}
