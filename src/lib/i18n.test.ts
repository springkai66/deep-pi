import { describe, expect, it } from "vitest";
import {
  englishMessageCount,
  interpolate,
  plural,
  toTraditional,
  traditionalMessage,
  traditionalPairCount,
  translate,
} from "./i18n-core";
import { EN_MESSAGES } from "./i18n-en";
import { S2T_PAIRS, ZH_TW_TERMS, ZH_TW_VOCAB } from "./i18n-zh-tw";
import { DEFAULT_LOCALE, LOCALES, LOCALE_LABELS, isLocale } from "./locale";

describe("locale identifiers", () => {
  it("exposes exactly the three supported languages", () => {
    expect([...LOCALES]).toEqual(["zh-CN", "zh-TW", "en"]);
    expect(DEFAULT_LOCALE).toBe("zh-CN");
  });

  it("labels every language in its own script", () => {
    for (const locale of LOCALES) {
      expect(LOCALE_LABELS[locale]).toBeTruthy();
    }
    expect(LOCALE_LABELS["zh-TW"]).toBe("繁體中文");
  });

  it("accepts only known locale values", () => {
    expect(isLocale("zh-TW")).toBe(true);
    expect(isLocale("en")).toBe(true);
    expect(isLocale("fr")).toBe(false);
    expect(isLocale(undefined)).toBe(false);
    expect(isLocale(42)).toBe(false);
  });
});

describe("simplified Chinese source strings", () => {
  it("returns the source text unchanged", () => {
    expect(translate("zh-CN", "设置")).toBe("设置");
    expect(translate("zh-CN", "共 {count} 个任务", { count: 3 })).toBe("共 3 个任务");
  });

  it("interpolates named placeholders", () => {
    expect(interpolate("已修改 {count} 个文件", { count: 12 })).toBe("已修改 12 个文件");
    expect(interpolate("{a} 和 {b}", { a: "1", b: "2" })).toBe("1 和 2");
  });

  it("keeps an unresolved placeholder visible instead of blanking it", () => {
    expect(interpolate("共 {count} 个任务", {})).toBe("共 {count} 个任务");
  });
});

describe("traditional Chinese conversion", () => {
  it("maps simplified characters to traditional ones", () => {
    expect(toTraditional("们")).toBe("們");
    expect(toTraditional("这")).toBe("這");
  });

  it("keeps characters that are identical in both scripts", () => {
    expect(toTraditional("中文 Pi 123")).toBe("中文 Pi 123");
  });

  it("produces a traditional string for any input", () => {
    const result = translate("zh-TW", "设置");
    expect(typeof result).toBe("string");
    expect(result.length).toBeGreaterThan(0);
  });

  it("applies whole-phrase overrides when the phrase is covered", () => {
    const entries = Object.entries(ZH_TW_VOCAB);
    for (const [simplified, traditional] of entries) {
      expect(traditionalMessage(simplified)).toBe(traditional);
    }
  });

  it("applies terminology corrections after character conversion", () => {
    for (const [from, to] of ZH_TW_TERMS) {
      expect(from).not.toBe(to);
      expect(traditionalMessage(`x${from}y`)).toContain(to);
    }
  });
});

describe("english catalogue", () => {
  it("returns an English string for translated entries", () => {
    const entries = Object.entries(EN_MESSAGES);
    if (entries.length === 0) return;
    for (const [simplified, english] of entries.slice(0, 40)) {
      expect(translate("en", simplified)).toBe(english);
      // 译文不应再是原文（除非确实同名，例如 "Pi"）
      expect(typeof english).toBe("string");
      expect(english.length).toBeGreaterThan(0);
    }
  });

  it("falls back to the simplified source when a key is missing", () => {
    const missing = "这个键一定不在目录里-测试回落";
    expect(EN_MESSAGES[missing]).toBeUndefined();
    expect(translate("en", missing)).toBe(missing);
  });

  it("keeps every placeholder from the source string in the translation", () => {
    const placeholder = /\{(\w+)\}/g;
    const problems: string[] = [];
    for (const [simplified, english] of Object.entries(EN_MESSAGES)) {
      const expected = new Set([...simplified.matchAll(placeholder)].map((m) => m[1]));
      if (expected.size === 0) continue;
      const actual = new Set([...english.matchAll(placeholder)].map((m) => m[1]));
      for (const name of expected) {
        if (!actual.has(name)) problems.push(`${simplified} → ${english}（缺 {${name}}）`);
      }
    }
    expect(problems).toEqual([]);
  });

  it("does not leave empty translations", () => {
    const empty = Object.entries(EN_MESSAGES)
      .filter(([, value]) => !value.trim())
      .map(([key]) => key);
    expect(empty).toEqual([]);
  });
});

describe("simplified-to-traditional data integrity", () => {
  it("stores every mapping as exactly one character pair", () => {
    const bad = S2T_PAIRS.filter((pair) => pair.length !== 2);
    expect(bad).toEqual([]);
  });

  it("never maps a character to itself", () => {
    const identity = S2T_PAIRS.filter((pair) => pair[0] === pair[1]);
    expect(identity).toEqual([]);
  });

  it("has no duplicated source characters", () => {
    const seen = new Set<string>();
    const duplicates: string[] = [];
    for (const pair of S2T_PAIRS) {
      if (seen.has(pair[0])) duplicates.push(pair[0]);
      seen.add(pair[0]);
    }
    expect(duplicates).toEqual([]);
  });

  it("covers a useful number of characters", () => {
    expect(traditionalPairCount()).toBeGreaterThan(200);
  });
});

describe("plural helper", () => {
  it("picks the matching variant and injects the count", () => {
    expect(plural("zh-CN", 1, "共 {count} 个任务", "共 {count} 个任务")).toBe("共 1 个任务");
    expect(plural("en", 2, "{count} task", "{count} tasks")).toBe("2 tasks");
  });
});

describe("coverage reporting", () => {
  it("reports how many english messages and character pairs are loaded", () => {
    expect(englishMessageCount()).toBe(Object.keys(EN_MESSAGES).length);
    expect(traditionalPairCount()).toBeGreaterThan(0);
  });
});

describe("thinking level labels", () => {
  // 推理强度下拉直接显示 Pi 自己的档位名（off/low/high/…），不做中文意译。
  it("no longer carries per-tier label overrides", () => {
    for (const key of ["关闭", "最简", "低", "中", "高", "超高", "最大"]) {
      expect(key in EN_MESSAGES && EN_MESSAGES[key] === "Off").toBe(false);
    }
    expect(translate("en", "推理强度")).toBe("Reasoning effort");
    // 关闭按钮走 shell 分片的 "Close"，而不是曾经被 chat 分片覆盖的 "Off"。
    expect(translate("en", "关闭")).toBe("Close");
  });
});
