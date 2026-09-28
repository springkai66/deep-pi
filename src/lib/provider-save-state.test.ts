import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

// 组件里保存相关逻辑是对 $state 的闭包操作（draftSnapshot/saveProvider），
// 本仓库无 jsdom 组件测试设施；这里用「源码不变式」锁住三类回归：
// 1) $derived 不得再调用 syncDraftHeaders()（在派生里写响应式状态曾让保存按钮失效）；
// 2) draftSnapshot 必须纯读取（不修改 draft）；
// 3) saveProvider 必须以提交时刻的草稿为准，且失败时不丢弃用户输入。

const source = readFileSync("src/lib/PiProviderSettings.svelte", "utf8");

/** 取一个顶层函数的源码片段（从声明到下一个顶层声明前）。 */
function functionSource(name: string): string {
  const start = source.indexOf(`function ${name}(`);
  expect(start, `function ${name}() should exist`).toBeGreaterThanOrEqual(0);
  const rest = source.slice(start);
  const next = rest.slice(1).search(/\n  (async )?function |\n  onMount\(/);
  return next < 0 ? rest : rest.slice(0, next);
}

describe("provider model save state", () => {
  it("does not mutate reactive state from the derived dirty check", () => {
    // draftSnapshot 在 $derived(draftDirty) 里求值，任何对 draft 的写入都会形成
    // 「求值→写入→再失效」循环，保存按钮的 dirty 判定因此不可靠。
    const snapshot = functionSource("draftSnapshot");
    expect(snapshot).not.toMatch(/syncDraftHeaders\(\)/);
    expect(snapshot).toMatch(/draftHeaders\(\)/); // headers 用纯函数取值，不落回 draft
  });

  it("builds the snapshot from a pure headers view", () => {
    const headers = functionSource("draftHeaders");
    expect(headers).toMatch(/Object\.fromEntries/);
    // draftHeaders 只读 headerEntries，不写 draft.headers
    expect(headers).not.toMatch(/draft\.headers\s*=/);
  });

  it("submits the captured draft and keeps user edits on failure", () => {
    const save = functionSource("saveProvider");
    // 保存以提交时刻的快照为准（异步期间的新编辑不被覆盖进请求）
    expect(save).toMatch(/submittedSnapshot = draftSnapshot\(\)/);
    expect(save).toMatch(/\.\.\.submitted\.draft/);
    // isLoading 卡死曾让保存按钮永远不可点：重入早退必须放在提交快照之后、不影响 finally
    expect(save).toMatch(/if \(isLoading\) return null;/);
    // 保存失败时草稿不被替换，用户输入保留可重试
    expect(save).toMatch(/catch \(error\) \{\s*\n\s*onError\(error\);\s*\n\s*return null;/);
  });

  it("re-baselines the saved snapshot with the same shape it compares against", () => {
    // savedSnapshot 与 draftSnapshot 必须是同一 JSON 形状，否则 draftDirty 恒真/恒假
    // 保存成功后的基线必须与 draftSnapshot 同形状（否则 draftDirty 恒真/恒假）。
    // selectProvider 里 savedSnapshot = draftSnapshot() 是合法基线（以当前草稿为准）。
    expect(source).toMatch(/savedSnapshot = JSON\.stringify\(\{ draft: saved, apiKey: "", providerPreset: submitted\.providerPreset \}\)/);
    // 保存失败路径不得改基线（保持 dirty 可重试）。
    const catchBlock = functionSource("saveProvider").slice(functionSource("saveProvider").indexOf("catch (error)"));
    expect(catchBlock.slice(0, 120)).toMatch(/onError\(error\);\s*\n\s*return null;/);
  });

  it("restores editable fields only when nothing changed during the save", () => {
    // 保存期间用户继续编辑 → draft 保持原样（不被 saved 覆盖）
    expect(source).toMatch(/if \(draft === submittedDraft\) \{/);
    expect(source).toMatch(/const unchanged = draftSnapshot\(\) === submittedSnapshot;/);
    expect(source).toMatch(/if \(unchanged\) \{\s*\n\s*draft = cloneProvider\(saved\);/);
  });
});
