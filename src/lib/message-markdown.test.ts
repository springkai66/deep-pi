import { describe, expect, it, vi } from "vitest";
import { confirmMessageLink, parseMessageMarkdown, safeMessageLink, type MarkdownNode } from "./message-markdown";

function flatten(nodes: MarkdownNode[]): MarkdownNode[] {
  return nodes.flatMap((node) => [node, ...("children" in node ? flatten(node.children) : [])]);
}

describe("message markdown", () => {
  it("does not open a link after its owning conversation changes during confirmation", async () => {
    let current = true;
    let answer!: (value: boolean) => void;
    const open = vi.fn(async () => {});
    const pending = confirmMessageLink("https://example.test", {
      current: () => current, open,
      confirm: () => new Promise((resolve) => { answer = resolve; }),
    });
    current = false;
    answer(true);
    expect(await pending).toBe(false);
    expect(open).not.toHaveBeenCalled();
  });
  it("requires explicit confirmation and uses the exact canonical destination", async () => {
    const open = vi.fn(async () => {});
    const confirm = vi.fn(async () => false);
    expect(await confirmMessageLink("https://example.test", { current: () => true, confirm, open })).toBe(false);
    expect(open).not.toHaveBeenCalled();
    confirm.mockResolvedValueOnce(true);
    expect(await confirmMessageLink("https://example.test", { current: () => true, confirm, open })).toBe(true);
    expect(confirm).toHaveBeenLastCalledWith("https://example.test/");
    expect(open).toHaveBeenCalledExactlyOnceWith("https://example.test/");
  });
  it("decodes Markdown entities once but preserves literal code entities", () => {
    const nodes = flatten(parseMessageMarkdown("&#60;script&#62; &amp; `&lt;` [link](https://example.test/?a=1&amp;b=2)").nodes);
    expect(nodes.find((node) => node.kind === "code")).toMatchObject({ content: "&lt;" });
    expect(nodes.filter((node) => node.kind === "text").map((node) => node.content).join("")).toContain("<script> &");
    expect(nodes.find((node) => node.kind === "element" && node.tag === "a")).toMatchObject({ href: "https://example.test/?a=1&b=2" });
  });
  it("rejects obfuscated schemes and whitespace while displaying canonical destinations", () => {
    for (const value of ["https:\\\\example.test", "https://example.test%0a.evil", "java%73cript:alert(1)",
      "https://example.test/\u00a0secret"]) expect(safeMessageLink(value)).toBeNull();
    expect(safeMessageLink("https://example.test/路径")).toBe("https://example.test/%E8%B7%AF%E5%BE%84");
    expect(safeMessageLink("https://example.test\\@other.test/path")).toBe("https://example.test/@other.test/path");
  });
  it("parses common formatting, nested lists, tables and fenced code while keeping code characters literal", () => {
    const result = parseMessageMarkdown("# 标题\n\n**加粗** 和 `x`\n\n1. one\n   - two\n\n| A | B |\n| - | - |\n| 1 | 2 |\n\n```ts\nconst x = '<script>';\n```\n");
    const nodes = flatten(result.nodes);
    expect(nodes.filter((node) => node.kind === "element").map((node) => node.tag))
      .toEqual(expect.arrayContaining(["h1", "strong", "ol", "ul", "table", "td"]));
    expect(nodes.find((node) => node.kind === "code" && node.block)).toMatchObject({
      content: "const x = '<script>';", language: "ts",
    });
    expect(result.plain).toBe(false);
  });
  it("never turns raw HTML, SVG or script source into executable elements", () => {
    const source = '<img src=x onerror="alert(1)">\n\n<svg onload=alert(2)></svg>\n\n<script>alert(3)</script>';
    const nodes = flatten(parseMessageMarkdown(source).nodes);
    expect(nodes.filter((node) => node.kind === "element").map((node) => node.tag))
      .not.toEqual(expect.arrayContaining(["script", "img", "svg"]));
    expect(nodes.filter((node) => node.kind === "text").map((node) => node.content).join("")).toContain("<script>");
  });
  it("allows only explicit credential-free HTTP(S) links, rejecting executable and local schemes", () => {
    for (const value of ["javascript:alert(1)", "data:text/html,x", "file:///C:/secret", "//host.test/a",
      "../a.ts", "#x", "https://user:secret@host.test", "https://host.test/\nsecret", "mailto:a@b.test"]) {
      expect(safeMessageLink(value), value).toBeNull();
    }
    expect(safeMessageLink("https://example.test/a?q=1#b")).toBe("https://example.test/a?q=1#b");
    const nodes = flatten(parseMessageMarkdown("[x](javascript&#58;alert(1)) [ok](https://example.test)").nodes);
    expect(nodes.flatMap((node) => node.kind === "element" && node.tag === "a" ? [node.href] : []))
      .toEqual(["https://example.test/"]);
  });
  it("represents remote images as explicit links without fetching them", () => {
    const nodes = flatten(parseMessageMarkdown('![private image](https://tracker.test/pixel.png "title")').nodes);
    expect(nodes.find((node) => node.kind === "image")).toMatchObject({
      content: "private image", href: "https://tracker.test/pixel.png",
    });
    expect(nodes.some((node) => node.kind === "element" && String(node.tag) === "img")).toBe(false);
  });
  it("preserves oversized content as plain text instead of silently truncating it", () => {
    const source = "# heading\n" + "长".repeat(300_000);
    expect(parseMessageMarkdown(source)).toEqual({ nodes: [{ kind: "text", content: source }], plain: true });
  });
  it("accepts incomplete streaming fences as code while keeping unsupported languages inert", () => {
    const nodes = flatten(parseMessageMarkdown('```unknown" onclick="evil\nhello <world>').nodes);
    expect(nodes.find((node) => node.kind === "code")).toMatchObject({ language: 'unknown"', content: "hello <world>" });
  });
});
