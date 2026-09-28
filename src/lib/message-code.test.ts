import { describe, expect, it } from "vitest";
import { highlightMessageCode } from "./message-code";
import { decode } from "he";

describe("message code highlighting", () => {
  it("preserves every character while producing only fixed-class spans across languages", async () => {
    const source = 'x = "<script>&\\""\r\n// 中文\n';
    for (const language of ["js", "python", "rust", "html", "__proto__", "constructor"]) {
      const html = await highlightMessageCode(source, language);
      expect(decode(html.replace(/<span class="[a-zA-Z0-9 -]+">|<\/span>/g, ""))).toBe(source);
      expect(html).not.toContain("<script>");
    }
  });
  it("highlights a registered language and escapes HTML inside code", async () => {
    const value = await highlightMessageCode('const value = "<script>alert(1)</script>";', "js");
    expect(value).toContain("tok-keyword");
    expect(value).not.toContain("<script>");
    expect(value).toContain("&lt;script&gt;");
  });
  it("uses escaped plain code for unknown and oversized languages without guessing", async () => {
    expect(await highlightMessageCode("<img onerror=x>", "not-a-language")).toBe("&lt;img onerror=x&gt;");
    const huge = "<".repeat(100_000);
    expect(await highlightMessageCode(huge, "js")).toBe("&lt;".repeat(100_000));
  });
});
