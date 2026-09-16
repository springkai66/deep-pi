import { describe, expect, it } from "vitest";
import {
  APP_MESSAGES,
  APP_MESSAGE_PREFIX,
  appConfirmDialog,
  appMessageCodes,
  appMessageText,
  isAppMessage,
  parseAppMessage,
  resolveAppMessage,
  type AppMessageText,
} from "./app-messages";
import { LOCALES } from "./locale";

describe("message detection", () => {
  it("recognises the backend message prefix", () => {
    expect(isAppMessage("@msg:runtime.dsh.pinned")).toBe(true);
    expect(isAppMessage("普通文本")).toBe(false);
    expect(isAppMessage("")).toBe(false);
  });
});

describe("message parsing", () => {
  it("parses a bare code", () => {
    expect(parseAppMessage("@msg:app.startup.not_ready")).toEqual({
      code: "app.startup.not_ready",
      params: {},
    });
  });

  it("parses ordered parameters", () => {
    const parsed = parseAppMessage("@msg:runtime.dsh.pinned?latest=0.2.0&pinned=0.1.5");
    expect(parsed?.code).toBe("runtime.dsh.pinned");
    expect(parsed?.params).toEqual({ latest: "0.2.0", pinned: "0.1.5" });
  });

  it("decodes percent-escaped values, including non-ASCII", () => {
    // Rust 侧把 `&`/`=`/`?` 与非 ASCII 逐字节转义，这里必须能还原
    const parsed = parseAppMessage("@msg:x.y?name=%E5%B7%A5%E4%BD%9C%E5%8C%BA&detail=a%26b%3Dc");
    expect(parsed?.params.name).toBe("工作区");
    expect(parsed?.params.detail).toBe("a&b=c");
  });

  it("survives malformed escapes instead of throwing", () => {
    const parsed = parseAppMessage("@msg:x.y?bad=%E5%B7");
    expect(parsed?.code).toBe("x.y");
    expect(typeof parsed?.params.bad).toBe("string");
  });

  it("returns null for plain text so callers keep the original", () => {
    expect(parseAppMessage("普通错误")).toBeNull();
    expect(parseAppMessage("Error: something failed")).toBeNull();
  });

  it("ignores malformed pairs", () => {
    const parsed = parseAppMessage("@msg:x.y?novalue&&empty=");
    expect(parsed?.params).toEqual({ empty: "" });
  });
});

describe("message rendering", () => {
  it("renders each registered code in every language", () => {
    for (const [code, entry] of Object.entries(APP_MESSAGES)) {
      for (const locale of LOCALES) {
        const text = resolveAppMessage(`${APP_MESSAGE_PREFIX}${code}`, locale);
        expect(text, `${code} / ${locale}`).toBeTruthy();
        expect(text).not.toContain(APP_MESSAGE_PREFIX);
      }
    }
  });

  it("substitutes parameters", () => {
    const text = resolveAppMessage("@msg:runtime.dsh.pinned?latest=0.2.0&pinned=0.1.5", "zh-CN");
    expect(text).toContain("0.2.0");
    expect(text).toContain("0.1.5");
    expect(text).not.toContain("{latest}");
    expect(text).not.toContain("{pinned}");
  });

  it("does not leak placeholders in any language", () => {
    const problems: string[] = [];
    for (const [code, entry] of Object.entries(APP_MESSAGES)) {
      const placeholders = new Set([...entry["zh-CN"].matchAll(/\{(\w+)\}/g)].map((m) => m[1]));
      for (const locale of LOCALES) {
        const text = entry[locale as keyof AppMessageText];
        for (const name of placeholders) {
          if (!text.includes(`{${name}}`)) problems.push(`${code} / ${locale} 缺 {${name}}`);
        }
      }
    }
    expect(problems).toEqual([]);
  });

  it("shows the code itself when a message is not registered yet", () => {
    // 漏配文案时要能立刻发现，而不是显示空白
    expect(resolveAppMessage("@msg:unknown.code", "en")).toBe("unknown.code");
  });

  it("passes non-messages through untouched", () => {
    expect(resolveAppMessage("普通错误", "en")).toBe("普通错误");
    expect(resolveAppMessage("Error: boom", "zh-TW")).toBe("Error: boom");
  });
});

describe("catalogue integrity", () => {
  it("registers at least the startup and runtime messages", () => {
    const codes = appMessageCodes();
    expect(codes).toContain("app.startup.not_ready");
    expect(codes).toContain("runtime.dsh.unsupported");
    expect(codes).toContain("runtime.dsh.pinned");
  });

  it("uses dotted lower-case codes made of letters, digits and underscores", () => {
    for (const code of appMessageCodes()) {
      expect(code, code).toMatch(/^[a-z][a-z0-9_]*(\.[a-z0-9_]+)+$/);
    }
  });

  it("has no empty translations", () => {
    const empty: string[] = [];
    for (const [code, entry] of Object.entries(APP_MESSAGES)) {
      for (const locale of LOCALES) {
        if (!entry[locale as keyof AppMessageText]?.trim()) empty.push(`${code} / ${locale}`);
      }
    }
    expect(empty).toEqual([]);
  });
});

/** 原生确认对话框的 10 个消息码前缀（见 `app-messages-dialogs.ts`）。 */
const CONFIRM_DIALOG_CODES = [
  "git.trust.dialog",
  "git.index.dialog.stage",
  "git.index.dialog.unstage",
  "git.index.dialog.resolve",
  "git.commit.dialog",
  "git.push.dialog",
  "git.sync.dialog",
  "recovery.delete.dialog.permanent",
  "recovery.delete.dialog.record_only",
  "recovery.restore.dialog",
] as const;

describe("native confirmation dialogs", () => {
  it("registers title/message/confirm/cancel for every dialog in all languages", () => {
    const problems: string[] = [];
    for (const code of CONFIRM_DIALOG_CODES) {
      for (const field of ["title", "message", "confirm", "cancel"]) {
        const key = `${code}.${field}`;
        expect(appMessageCodes(), key).toContain(key);
        for (const locale of LOCALES) {
          const text = appMessageText(key, locale);
          if (!text.trim()) problems.push(`${key} / ${locale} 为空`);
          if (text === key) problems.push(`${key} / ${locale} 未登记`);
        }
      }
    }
    expect(problems).toEqual([]);
  });

  it("registers the dialogue terms with their placeholders", () => {
    const termCodes = [
      "git.commit.dialog.term.signature_on",
      "git.commit.dialog.term.signature_off",
      "git.commit.dialog.term.more_paths",
      "git.sync.dialog.term.new_ref",
    ];
    for (const code of termCodes) {
      for (const locale of LOCALES) {
        const text = appMessageText(code, locale);
        expect(text, `${code} / ${locale}`).toBeTruthy();
        expect(text, `${code} / ${locale}`).not.toBe(code);
      }
    }
    for (const locale of LOCALES) {
      expect(appMessageText("git.commit.dialog.term.more_paths", locale), locale).toContain(
        "{count}",
      );
      expect(appMessageText("app.dialog_text_invalid", locale), locale).toContain("{field}");
    }
  });

  it("assembles the payload shape expected by the Rust ConfirmDialog", () => {
    const payload = appConfirmDialog("git.commit.dialog", "en", [
      "signature_on",
      "signature_off",
      "more_paths",
    ]);
    expect(Object.keys(payload).sort()).toEqual([
      "cancelLabel",
      "confirmLabel",
      "message",
      "terms",
      "title",
    ]);
    expect(payload.title).toBe(appMessageText("git.commit.dialog.title", "en"));
    expect(payload.message).toBe(appMessageText("git.commit.dialog.message", "en"));
    expect(payload.confirmLabel).toBe("Commit");
    expect(payload.cancelLabel).toBe("Cancel");
    expect(Object.keys(payload.terms).sort()).toEqual([
      "more_paths",
      "signature_off",
      "signature_on",
    ]);
    const morePaths = appMessageText("git.commit.dialog.term.more_paths", "en");
    expect(payload.terms.more_paths).toBe(morePaths);
    expect(payload.terms.more_paths).toContain("{count}");
    expect(payload.terms.more_paths).not.toMatch(/[\u4e00-\u9fff]/u);
  });

  it("keeps the code visible when a text is not registered", () => {
    expect(appMessageText("不存在的码", "en")).toBe("不存在的码");
  });
});

/**
 * 「前端对话框文案 ↔ 后端渲染键」白名单。
 *
 * 后端只按 `src-tauri/src/dialog_text.rs` 里 `render(&[...])` / `render_term(...)`
 * 传入的键名替换 `{name}`；前端文案多写、少写或拼错一个占位符，界面就会漏出
 * 裸占位符（未命中的 `{...}` 会原样保留），因此这里把每个键的占位符集合钉死。
 */
const DIALOG_MESSAGE_PLACEHOLDERS: Record<string, readonly string[]> = {
  "git.trust.dialog.message": ["project", "worktree", "git_dir", "common_dir"],
  "git.index.dialog.stage.message": ["worktree", "paths"],
  "git.index.dialog.unstage.message": ["worktree", "paths"],
  "git.index.dialog.resolve.message": ["worktree", "paths"],
  "git.commit.dialog.message": [
    "worktree",
    "target",
    "author",
    "signature",
    "tree",
    "paths",
    "summary",
  ],
  "git.push.dialog.message": ["worktree", "remote", "destination", "target", "source"],
  "git.sync.dialog.message": [
    "worktree",
    "remote",
    "destination",
    "branch",
    "query",
    "references",
  ],
  "recovery.delete.dialog.permanent.message": ["project", "copy", "target"],
  "recovery.delete.dialog.record_only.message": ["project", "copy", "target"],
  "recovery.restore.dialog.message": ["project", "copy", "new_path"],
  "app.dialog_text_invalid": ["field"],
};

/** 4 个词汇模板的占位符白名单（后端用 `render_term` 渲染）。 */
const DIALOG_TERM_PLACEHOLDERS: Record<string, readonly string[]> = {
  "git.commit.dialog.term.signature_on": [],
  "git.commit.dialog.term.signature_off": [],
  "git.commit.dialog.term.more_paths": ["count"],
  "git.sync.dialog.term.new_ref": [],
};

/** 文案里出现的 `{name}` 去重后排序，便于与白名单整体比较。 */
function dialogPlaceholders(text: string): string[] {
  return [...new Set([...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]))].sort();
}

describe("dialog placeholder contract with the Rust renderer", () => {
  it("pins the placeholder set of every dialog message and term in all languages", () => {
    const problems: string[] = [];
    for (const [code, expected] of Object.entries({
      ...DIALOG_MESSAGE_PLACEHOLDERS,
      ...DIALOG_TERM_PLACEHOLDERS,
    })) {
      const want = [...expected].sort().join(", ");
      for (const locale of LOCALES) {
        const got = dialogPlaceholders(appMessageText(code, locale)).join(", ");
        if (got !== want) {
          problems.push(`${code} / ${locale}: 期望 [${want}]，实际 [${got}]`);
        }
      }
    }
    expect(problems).toEqual([]);
  });
});
