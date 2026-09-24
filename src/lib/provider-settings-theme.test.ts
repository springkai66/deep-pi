import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

// 模型设置页亮色适配回归锁：两个组件必须走主题令牌，否则亮色主题下会整块发黑。
// 1) <style> 块内不得再有写死的十六进制颜色；
// 2) 引用的每个 var(--token) 都必须在 app.css 里有定义；
// 3) 颜色令牌还必须在亮色主题块（:root[data-color-scheme="light"]）里被覆盖。
const COMPONENTS = [
  "src/lib/PiProviderSettings.svelte",
  "src/lib/PiAuthSettings.svelte",
  "src/lib/PiOfficialLoginFlow.svelte",
];
const APP_CSS = readFileSync("src/app.css", "utf8");
// 排版/布局令牌与主题无关，只在根块定义一次。
const THEME_INDEPENDENT_TOKENS = new Set([
  "app-font",
  "text-font",
  "session-font",
  "code-font",
  "topbar-h",
]);

function styleBlock(path: string): string {
  const source = readFileSync(path, "utf8");
  const start = source.indexOf("<style>");
  expect(start, path + " 应有 <style> 块").toBeGreaterThanOrEqual(0);
  return source.slice(start);
}

function referencedTokens(style: string): string[] {
  return [...style.matchAll(/var\(--([a-zA-Z0-9-]+)/g)].map((match) => match[1]);
}

function definedTokens(css: string): Set<string> {
  return new Set([...css.matchAll(/--([a-zA-Z0-9-]+)\s*:/g)].map((match) => match[1]));
}

describe("模型设置页主题令牌", () => {
  it("样式里不再出现写死的十六进制颜色", () => {
    for (const path of COMPONENTS) {
      const hex = [...styleBlock(path).matchAll(/#[0-9a-fA-F]{3,8}\b/g)].map((match) => match[0]);
      expect(hex, path + " 的样式仍含写死颜色：亮色主题下会呈现深色块").toEqual([]);
    }
  });

  it("引用的令牌都在 app.css 中有定义", () => {
    const defined = definedTokens(APP_CSS);
    const missing: string[] = [];
    for (const path of COMPONENTS) {
      for (const token of referencedTokens(styleBlock(path))) {
        if (!defined.has(token)) missing.push(path + ": --" + token);
      }
    }
    expect(missing).toEqual([]);
  });

  it("颜色令牌都被亮色主题覆盖（浅底深字）", () => {
    const lightStart = APP_CSS.indexOf(':root[data-color-scheme="light"]');
    expect(lightStart, "app.css 应有亮色主题块").toBeGreaterThanOrEqual(0);
    const lightEnd = APP_CSS.indexOf("}", lightStart);
    const lightTokens = definedTokens(APP_CSS.slice(lightStart, lightEnd));
    const missing: string[] = [];
    for (const path of COMPONENTS) {
      for (const token of referencedTokens(styleBlock(path))) {
        if (THEME_INDEPENDENT_TOKENS.has(token)) continue;
        if (!lightTokens.has(token)) missing.push(path + ": --" + token);
      }
    }
    expect(missing).toEqual([]);
  });
});
